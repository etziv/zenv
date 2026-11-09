# syntax=docker/dockerfile:1
# Dockerfile - Go web service. Copy to docker/Dockerfile. Multi-stage: compile with Go caches,
# ship a small runtime. PKG = path of the main package (e.g. ./cmd/server); default: repo root.
# The context is the repo root, filtered by docker/Dockerfile.dockerignore (allow-list, copy
# Dockerfile.go.dockerignore): gitignored private data never enters the build.

FROM golang:1-alpine AS build
ARG PKG=.
# Compiler jobs: the build shares the server with production (and maybe its database). deploy.sh sets it.
ARG BUILD_JOBS=4
# -p caps parallel packages, GOMAXPROCS the threads of each compile
ENV GOMAXPROCS=${BUILD_JOBS}
WORKDIR /src
COPY go.mod go.sum* ./
RUN --mount=type=cache,target=/go/pkg/mod go mod download
COPY . .
# Cache mounts: a code change recompiles only the changed packages.
RUN --mount=type=cache,target=/go/pkg/mod \
    --mount=type=cache,target=/root/.cache/go-build \
    CGO_ENABLED=0 go build -p "${BUILD_JOBS}" -trimpath -ldflags="-s -w" -o /app-bin "${PKG}"

FROM alpine:3 AS app
RUN apk add --no-cache ca-certificates tzdata curl \
    && adduser -D -u 10001 app
COPY --from=build /app-bin /usr/local/bin/app
USER app
ENV APP_ADDR=0.0.0.0:8080
EXPOSE 8080
# Exec form: the binary is PID 1 and receives SIGTERM directly (graceful shutdown, see SKILL.md).
CMD ["/usr/local/bin/app"]
