package http

import (
	"net/http"
	"strings"
	"time"

	"github.com/gin-contrib/sessions"
	"github.com/gin-gonic/gin"
)

func RequestLogger() gin.HandlerFunc {
	return func(c *gin.Context) {
		start := time.Now()
		c.Next()
		latency := time.Since(start)
		_ = latency
	}
}

// Auth molto semplice: richiede "Authorization: Bearer <token>"
// In produzione useresti JWT/OPA/OAuth2 ecc.
func BearerAuth(expectedToken string) gin.HandlerFunc {
	return func(c *gin.Context) {
		h := c.GetHeader("Authorization")
		if !strings.HasPrefix(h, "Bearer ") {
			writeError(c, http.StatusUnauthorized, "UNAUTHORIZED", "missing bearer token")
			c.Abort()
			return
		}
		token := strings.TrimPrefix(h, "Bearer ")
		if token != expectedToken {
			writeError(c, http.StatusUnauthorized, "UNAUTHORIZED", "invalid token")
			c.Abort()
			return
		}
		c.Next()
	}
}

// AuthMiddleware controlla che l'utente sia loggato via sessione
func AuthMiddleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		session := sessions.Default(c)
		userID := session.Get("userID")

		if userID == nil {
			c.Redirect(http.StatusFound, "/login?error=Sessione scaduta. Effettuare il login.")
			c.Abort()
			return
		}

		// Aggiungere l'userID al contesto per accedervi successivamente
		c.Set("userID", userID)
		c.Next()
	}
}

// FormsIndex serve la pagina index con i link ai form
func FormsIndex(c *gin.Context) {
	session := sessions.Default(c)
	userID := session.Get("userID")
	c.HTML(http.StatusOK, "index.html", gin.H{
		"userID": userID,
	})
}

// ServeFormWithUserID serve un form HTML con lo userID
func ServeFormWithUserID(templateName string) gin.HandlerFunc {
	return func(c *gin.Context) {
		session := sessions.Default(c)
		userID := session.Get("userID")
		c.HTML(http.StatusOK, templateName, gin.H{
			"userID": userID,
		})
	}
}

// ServeCommonCSS serve il file CSS comune
func ServeCommonCSS(c *gin.Context) {
	c.File("./internal/api/http/templates/common.css")
}
