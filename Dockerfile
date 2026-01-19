# Multi-stage Dockerfile for koin

# Builder: build a static Go binary
FROM golang:1.25.6-alpine AS builder

# Allow setting the app version at build time; fallback to VERSION file.
ARG VERSION

# Install git (needed if go modules fetch from VCS)
RUN apk add --no-cache git

WORKDIR /src

# Download modules first (cached layer)
COPY go.mod go.sum ./
RUN go mod download

# Copy the rest and build
COPY . .
# Static build (CGO disabled) for linux/amd64
RUN app_version="${VERSION:-$(cat VERSION 2>/dev/null || echo dev)}" && \
    CGO_ENABLED=0 GOOS=linux GOARCH=amd64 \
    go build -ldflags="-s -w -X koin/internal/version.Version=${app_version}" -o /koin ./main.go

# Runtime image
FROM alpine:3.18
RUN apk add --no-cache ca-certificates

# Set working directory so relative paths in the app resolve correctly
WORKDIR /app

# Copy binary from builder
COPY --from=builder /koin /app/koin

# Copy migrations so golang-migrate can read them from the expected path
COPY --from=builder /src/internal/db/migrations ./internal/db/migrations
# Copy HTML templates used by Gin
COPY --from=builder /src/internal/api/http/templates ./internal/api/http/templates

EXPOSE 8080

ENTRYPOINT ["/app/koin"]
