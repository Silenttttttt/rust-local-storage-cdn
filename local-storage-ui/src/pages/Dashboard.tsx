import { Grid, Paper, Typography, CircularProgress, Box, Alert } from '@mui/material';
import { useQuery } from '@tanstack/react-query';
import { getStorageStats } from '../api/client';
import { formatBytes } from '../utils/format';

function StatCard({ label, value }: { label: string; value: string | number }) {
  return (
    <Grid item xs={12} sm={6} md={3}>
      <Paper sx={{ p: 2, textAlign: 'center' }}>
        <Typography variant="h6" gutterBottom color="text.secondary">
          {label}
        </Typography>
        <Typography variant="h4">{value}</Typography>
      </Paper>
    </Grid>
  );
}

export default function Dashboard() {
  const { data: stats, isLoading, error } = useQuery({
    queryKey: ['storage-stats'],
    queryFn: getStorageStats,
  });

  if (isLoading) {
    return (
      <Box display="flex" justifyContent="center" alignItems="center" minHeight="50vh">
        <CircularProgress />
      </Box>
    );
  }

  if (error || !stats) {
    return (
      <Box sx={{ p: 3 }}>
        <Alert severity="error">
          Failed to load storage statistics{error instanceof Error ? `: ${error.message}` : ''}
        </Alert>
      </Box>
    );
  }

  const avgFileSize = stats.total_files > 0 ? stats.total_size / stats.total_files : 0;

  return (
    <Box sx={{ p: 3 }}>
      <Typography variant="h4" gutterBottom>
        Storage Dashboard
      </Typography>
      <Grid container spacing={3}>
        <StatCard label="Total Files" value={stats.total_files.toLocaleString()} />
        <StatCard label="Total Size" value={formatBytes(stats.total_size)} />
        <StatCard label="Average File Size" value={formatBytes(avgFileSize)} />
        <StatCard label="Compressed Files" value={stats.compressed_files.toLocaleString()} />
        <StatCard label="Encrypted Files" value={stats.encrypted_files.toLocaleString()} />
        <StatCard
          label="Compression Ratio"
          value={stats.compression_ratio ? `${(stats.compression_ratio * 100).toFixed(1)}%` : 'N/A'}
        />
      </Grid>
    </Box>
  );
}
