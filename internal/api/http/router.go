package http

import (
	apigen "koin/internal/api/generated"
	"koin/internal/service"

	"github.com/gin-contrib/sessions"
	"github.com/gin-contrib/sessions/cookie"
	"github.com/gin-gonic/gin"
)

type RouterDeps struct {
	AuthToken   string
	Controller  apigen.ServerInterface // importante: dipendenza sul contratto generato
	UserService *service.UserService
}

func NewRouter(deps RouterDeps) *gin.Engine {
	r := gin.New()
	r.Use(gin.Recovery())
	r.Use(RequestLogger())

	// Carica i template HTML
	r.LoadHTMLGlob("./internal/api/http/templates/*.html")

	// Setup sessioni
	store := cookie.NewStore([]byte("secret-key"))
	r.Use(sessions.Sessions("koin-session", store))

	// Rotte pubbliche (senza autenticazione)
	r.GET("/login", LoginPage)
	r.POST("/login", func(c *gin.Context) {
		HandleLogin(c, deps.UserService)
	})
	r.GET("/signup", SignupPage)
	r.POST("/signup", func(c *gin.Context) {
		HandleSignup(c, deps.UserService)
	})
	r.GET("/logout", Logout)

	r.GET("/health", Health)

	// Serve static CSS file
	r.GET("/forms/common.css", ServeCommonCSS)

	// Form pages - protette da autenticazione
	protected := r.Group("/forms")
	protected.Use(AuthMiddleware())
	{
		protected.GET("", FormsIndex)
		protected.GET("/transactions", ServeFormWithUserID("transaction_form.html"))
		protected.GET("/accounts", ServeFormWithUserID("account_form.html"))
		protected.GET("/categories", ServeFormWithUserID("category_form.html"))
	}

	api := r.Group("/api")
	//api.Use(BearerAuth(deps.AuthToken))

	// per registrare tutti gli endpoint di quel controller
	apigen.RegisterHandlers(api, deps.Controller)

	return r
}
