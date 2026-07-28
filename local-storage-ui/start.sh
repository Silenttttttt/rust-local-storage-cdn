#!/bin/sh
# Inject API URL into config.js at container startup.
# REACT_APP_API_URL is what the deployment actually sets (CRA-style naming),
# API_URL is kept as a fallback for anything setting the shorter name.
API_URL="${REACT_APP_API_URL:-${API_URL:-http://localhost:8080}}"
cat > /usr/share/nginx/html/config.js << EOF
window.APP_CONFIG = { API_URL: '$API_URL' };
EOF
exec nginx -g 'daemon off;'
