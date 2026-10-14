package http

import (
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
)

func RequestLogger() gin.HandlerFunc {
	return func(c *gin.Context) {
		start := time.Now()
		c.Next()
		_ = time.Since(start)
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
