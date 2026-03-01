# Build stage optimized for CI caching
FROM node:20-slim AS builder

WORKDIR /app

# Copy package.json for dependency layer caching
COPY package.json ./

# Install dependencies (CI cache will preserve node_modules)
RUN npm install --silent

# Copy source code (separate layer for source changes)
COPY src ./src
COPY public ./public
COPY tsconfig.json ./

# Build the app with optimizations
ENV NODE_ENV=production
ENV GENERATE_SOURCEMAP=false
RUN npm run build

# Production stage
FROM nginx:alpine

# Install bash for script execution
RUN apk add --no-cache bash

# Create nginx directories with proper permissions
RUN mkdir -p /tmp/nginx/logs \
    && mkdir -p /tmp/nginx/tmp \
    && mkdir -p /tmp/nginx/client_temp \
    && mkdir -p /tmp/nginx/proxy_temp \
    && mkdir -p /tmp/nginx/fastcgi_temp \
    && mkdir -p /tmp/nginx/uwsgi_temp \
    && mkdir -p /tmp/nginx/scgi_temp \
    && chown -R nginx:nginx /tmp/nginx \
    && chmod -R 755 /tmp/nginx \
    && mkdir -p /var/cache/nginx \
    && chown -R nginx:nginx /var/cache/nginx \
    && chmod -R 755 /var/cache/nginx

# Copy nginx config
COPY nginx.conf /etc/nginx/nginx.conf

# Copy built files
COPY --from=builder /app/build /usr/share/nginx/html

# Fix ownership so nginx user can write to the directory
RUN chown -R nginx:nginx /usr/share/nginx/html \
    && chmod -R 755 /usr/share/nginx/html

# Copy startup script
COPY start.sh /start.sh
RUN chmod +x /start.sh

# Switch to nginx user after all setup is done
USER nginx

# Expose port
EXPOSE 3000

# Start with configuration script
CMD ["/start.sh"] 