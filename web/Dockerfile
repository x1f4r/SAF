# syntax=docker/dockerfile:1

# ---- build the SPA ----
FROM node:22-alpine AS build
WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm install
COPY . .
RUN npm run build

# ---- runtime: gateway + static SPA ----
FROM node:22-alpine
RUN apk add --no-cache openssh-client
WORKDIR /app
ENV NODE_ENV=production
COPY package.json package-lock.json* ./
RUN npm install --omit=dev
COPY server ./server
COPY --from=build /app/dist ./dist
EXPOSE 8090
CMD ["node", "server/index.mjs"]
