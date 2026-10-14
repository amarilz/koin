package http

import (
	"net/http"

	apigen "koin/internal/api/generated"
	"koin/internal/repository"

	"github.com/gin-gonic/gin"
)

type RouterDeps struct {
	Controller apigen.ServerInterface
	Recordings repository.VoiceRecordingRepository
}

func NewRouter(deps RouterDeps) *gin.Engine {
	r := gin.New()
	r.Use(gin.Recovery())
	r.Use(RequestLogger())
	r.LoadHTMLGlob("./internal/api/http/templates/*.html")

	r.GET("/", func(c *gin.Context) {
		c.Redirect(http.StatusFound, "/forms")
	})
	r.GET("/health", Health)
	r.GET("/forms/common.css", ServeCommonCSS)
	r.GET("/forms", FormsIndex)
	r.GET("/forms/transactions", ServeForm("transaction_form.html"))
	r.GET("/forms/accounts", ServeForm("account_form.html"))
	r.GET("/forms/categories", ServeForm("category_form.html"))
	r.GET("/forms/recordings", ServeForm("recording_form.html"))
	r.POST("/forms/recordings", createVoiceRecordingHandler(deps.Recordings))

	api := r.Group("/api")
	apigen.RegisterHandlers(api, deps.Controller)
	return r
}
