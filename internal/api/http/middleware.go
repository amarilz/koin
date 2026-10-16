package http

import (
	"log"
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
)

func RequestLogger() gin.HandlerFunc {
	return func(c *gin.Context) {
		start := time.Now()
		c.Next()
		log.Printf(
			"HTTP request | method=%s | path=%s | status=%d | duration=%s | client=%s",
			c.Request.Method,
			c.Request.URL.Path,
			c.Writer.Status(),
			time.Since(start).Round(time.Millisecond),
			c.ClientIP(),
		)
	}
}

func FormsIndex(c *gin.Context) {
	c.HTML(http.StatusOK, "index.html", nil)
}

func ServeForm(templateName string) gin.HandlerFunc {
	return func(c *gin.Context) {
		c.HTML(http.StatusOK, templateName, nil)
	}
}

func ServeCommonCSS(c *gin.Context) {
	c.File("./internal/api/http/templates/common.css")
}
