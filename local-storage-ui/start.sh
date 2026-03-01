#!/bin/sh
# Inject API URL into config.js at container startup
API_URL="${API_URL:-http://localhost:8080}"
cat > /usr/share/nginx/html/config.js << EOF
window.APP_CONFIG = { API_URL: '$API_URL' };
EOF
exec nginx -g 'daemon off;'
