FROM node:22-alpine
WORKDIR /app
COPY package*.json ./
RUN npm ci --omit=dev --no-audit --no-fund
COPY gateway.mjs ./
USER node
EXPOSE 8093
CMD ["node", "gateway.mjs"]
