package http

import (
	"net/http"

	apigen "koin/internal/api/generated"
	"koin/internal/repository"
	"koin/internal/service"

	"github.com/gin-gonic/gin"
)

type RouterDeps struct {
	Controller  apigen.ServerInterface
	Recordings  repository.VoiceRecordingReviewRepository
	Transcriber *service.VoiceTranscriptionClient
	Accounts    *service.AccountService
	Extractor   *service.TransactionExtractionClient
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
	r.POST("/forms/recordings", createVoiceRecordingHandler(deps.Recordings, deps.Transcriber))
	review := newVoiceRecordingReviewHandler(deps.Recordings, deps.Accounts, deps.Extractor)
	r.GET("/forms/recordings/reviewable", review.list)
	r.GET("/forms/recordings/:id/audio", review.audio)
	r.POST("/forms/recordings/:id/extract", review.extract)
	r.POST("/forms/recordings/:id/confirm", review.confirm)
	r.DELETE("/forms/recordings/:id", review.delete)

	api := r.Group("/api")
	apigen.RegisterHandlers(api, deps.Controller)
	return r
}
