import { Grid, Paper, Typography, CircularProgress, Box, Alert } from '@mui/material';
import { useQuery } from '@tanstack/react-query';
import { getStorageStats } from '../api/client';
import { formatBytes } from '../utils/format';

export default function Dashboard() {
  const { data: stats, isLoading, error } = useQuery({
    queryKey: ['storage-stats'],
    queryFn: getStorageStats,
  });

  // Debug information
  const apiUrl = window.APP_CONFIG?.API_URL || process.env.REACT_APP_API_URL || 'http://localhost:8080';

  if (isLoading) {
    return (
      <Box>
        <Alert severity="info" sx={{ mb: 2 }}>
          🔧 Debug Info: API URL = {apiUrl}
        </Alert>
        <Box display="flex" justifyContent="center" alignItems="center" minHeight="50vh">
          <CircularProgress />
        </Box>
      </Box>
    );
  }

  if (error) {
    return (
      <Box>
        <Alert severity="error" sx={{ mb: 2 }}>
          ❌ Connection Error: {error.message}
        </Alert>
        <Alert severity="info" sx={{ mb: 2 }}>
          🔧 Debug Info: API URL = {apiUrl}
        </Alert>
        <Typography color="error">
          Failed to load storage statistics
        </Typography>
      </Box>
    );
  }

  if (!stats) {
    return (
      <Box>
        <Alert severity="warning" sx={{ mb: 2 }}>
          ⚠️ No data received from API
        </Alert>
        <Alert severity="info" sx={{ mb: 2 }}>
          🔧 Debug Info: API URL = {apiUrl}
        </Alert>
        <Typography color="error">
          Failed to load storage statistics
        </Typography>
      </Box>
    );
  }

  // Calculate average file size if we have files
  const avgFileSize = stats.total_files > 0 ? stats.total_size / stats.total_files : 0;

  return (
    <Grid container spacing={3}>
      <Grid item xs={12}>
        <Typography variant="h4" gutterBottom>
          Storage Dashboard
        </Typography>
      </Grid>
      <Grid item xs={12} sm={6} md={3}>
        <Paper sx={{ p: 2, textAlign: 'center' }}>
          <Typography variant="h6" gutterBottom>
            Total Files
          </Typography>
          <Typography variant="h4">
            {stats.total_files.toLocaleString()}
          </Typography>
        </Paper>
      </Grid>
      <Grid item xs={12} sm={6} md={3}>
        <Paper sx={{ p: 2, textAlign: 'center' }}>
          <Typography variant="h6" gutterBottom>
            Total Size
          </Typography>
          <Typography variant="h4">
            {formatBytes(stats.total_size)}
          </Typography>
        </Paper>
      </Grid>
      <Grid item xs={12} sm={6} md={3}>
        <Paper sx={{ p: 2, textAlign: 'center' }}>
          <Typography variant="h6" gutterBottom>
            Average File Size
          </Typography>
          <Typography variant="h4">
            {formatBytes(avgFileSize)}
          </Typography>
        </Paper>
      </Grid>
      <Grid item xs={12} sm={6} md={3}>
        <Paper sx={{ p: 2, textAlign: 'center' }}>
          <Typography variant="h6" gutterBottom>
            Compressed Files
          </Typography>
          <Typography variant="h4">
            {stats.compressed_files.toLocaleString()}
          </Typography>
        </Paper>
      </Grid>
      <Grid item xs={12} sm={6} md={3}>
        <Paper sx={{ p: 2, textAlign: 'center' }}>
          <Typography variant="h6" gutterBottom>
            Encrypted Files
          </Typography>
          <Typography variant="h4">
            {stats.encrypted_files.toLocaleString()}
          </Typography>
        </Paper>
      </Grid>
      <Grid item xs={12} sm={6} md={3}>
        <Paper sx={{ p: 2, textAlign: 'center' }}>
          <Typography variant="h6" gutterBottom>
            Compression Ratio
          </Typography>
          <Typography variant="h4">
            {stats.compression_ratio ? (stats.compression_ratio * 100).toFixed(1) + '%' : 'N/A'}
          </Typography>
        </Paper>
      </Grid>
    </Grid>
  );
} 