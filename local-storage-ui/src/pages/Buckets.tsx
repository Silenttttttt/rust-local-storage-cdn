import React, { useState } from 'react';
import {
  Box, Card, CardContent, Typography, Grid, IconButton, Dialog, DialogTitle, DialogContent, 
  DialogActions, Button, List, ListItem, ListItemText, ListItemSecondaryAction, Alert,
  Menu, MenuItem, Divider, ListItemIcon, Avatar, Chip
} from '@mui/material';
import {
  Folder, MoreVert, Delete, Storage, Visibility, Analytics
} from '@mui/icons-material';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate } from 'react-router-dom';
import { listBuckets, deleteBucket, getBucketStats } from '../api/client';
import { formatBytes } from '../utils/format';

export default function Buckets() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [bucketToDelete, setBucketToDelete] = useState<string | null>(null);
  const [anchorEl, setAnchorEl] = useState<null | HTMLElement>(null);
  const [menuBucket, setMenuBucket] = useState<string | null>(null);

  // Queries
  const { data: buckets = [], isLoading, error } = useQuery({
    queryKey: ['buckets'],
    queryFn: listBuckets,
  });

  // Fetch stats for each bucket
  const bucketStatsQueries = useQuery({
    queryKey: ['all-bucket-stats', buckets],
    queryFn: async () => {
      if (!buckets.length) return {};
      const statsPromises = buckets.map(async (bucket) => {
        try {
          const stats = await getBucketStats(bucket);
          return { bucket, stats };
        } catch (error) {
          return { bucket, stats: null };
        }
      });
      const results = await Promise.all(statsPromises);
      return results.reduce((acc, { bucket, stats }) => {
        acc[bucket] = stats;
        return acc;
      }, {} as Record<string, any>);
    },
    enabled: buckets.length > 0,
  });

  // Mutations
  const deleteMutation = useMutation({
    mutationFn: deleteBucket,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['buckets'] });
      queryClient.invalidateQueries({ queryKey: ['all-bucket-stats'] });
      setDeleteDialogOpen(false);
      setBucketToDelete(null);
    },
  });

  // Handlers
  const handleMenuClick = (event: React.MouseEvent<HTMLElement>, bucket: string) => {
    setAnchorEl(event.currentTarget);
    setMenuBucket(bucket);
  };

  const handleMenuClose = () => {
    setAnchorEl(null);
    setMenuBucket(null);
  };

  const handleDelete = (bucket: string) => {
    setBucketToDelete(bucket);
    setDeleteDialogOpen(true);
    handleMenuClose();
  };

  const handleView = (bucket: string) => {
    navigate(`/buckets/${bucket}`);
    handleMenuClose();
  };

  const confirmDelete = () => {
    if (bucketToDelete) {
      deleteMutation.mutate(bucketToDelete);
    }
  };

  const totalFiles = Object.values(bucketStatsQueries.data || {}).reduce(
    (acc: number, stats: any) => acc + (stats?.total_files || 0), 0
  );

  const totalSize = Object.values(bucketStatsQueries.data || {}).reduce(
    (acc: number, stats: any) => acc + (stats?.total_size || 0), 0
  );

  const totalCompressed = Object.values(bucketStatsQueries.data || {}).reduce(
    (acc: number, stats: any) => acc + (stats?.compressed_files || 0), 0
  );

  return (
    <Box sx={{ p: 3 }}>
      <Typography variant="h4" gutterBottom sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
        <Storage color="primary" />
        Buckets ({buckets.length})
      </Typography>

      {/* Summary Stats */}
      <Grid container spacing={2} sx={{ mb: 3 }}>
        <Grid item xs={12} sm={6} md={3}>
          <Card>
            <CardContent sx={{ textAlign: 'center' }}>
              <Typography color="textSecondary" gutterBottom>Total Buckets</Typography>
              <Typography variant="h4">{buckets.length}</Typography>
            </CardContent>
          </Card>
        </Grid>
        <Grid item xs={12} sm={6} md={3}>
          <Card>
            <CardContent sx={{ textAlign: 'center' }}>
              <Typography color="textSecondary" gutterBottom>Total Files</Typography>
              <Typography variant="h4">{totalFiles.toLocaleString()}</Typography>
            </CardContent>
          </Card>
        </Grid>
        <Grid item xs={12} sm={6} md={3}>
          <Card>
            <CardContent sx={{ textAlign: 'center' }}>
              <Typography color="textSecondary" gutterBottom>Total Size</Typography>
              <Typography variant="h4">{formatBytes(totalSize)}</Typography>
            </CardContent>
          </Card>
        </Grid>
        <Grid item xs={12} sm={6} md={3}>
          <Card>
            <CardContent sx={{ textAlign: 'center' }}>
              <Typography color="textSecondary" gutterBottom>Compressed Files</Typography>
              <Typography variant="h4">{totalCompressed.toLocaleString()}</Typography>
            </CardContent>
          </Card>
        </Grid>
      </Grid>

      {/* Error Message */}
      {error && (
        <Alert severity="error" sx={{ mb: 2 }}>
          Failed to load buckets: {error.message}
        </Alert>
      )}

      {deleteMutation.error && (
        <Alert severity="error" sx={{ mb: 2 }}>
          Failed to delete bucket: {deleteMutation.error.message}
        </Alert>
      )}

      {/* Buckets List */}
      <Card>
        <CardContent>
          <Typography variant="h6" sx={{ mb: 2, display: 'flex', alignItems: 'center', gap: 1 }}>
            <Folder />
            Your Buckets
          </Typography>
          
          {isLoading ? (
            <Typography>Loading buckets...</Typography>
          ) : buckets.length === 0 ? (
            <Box sx={{ textAlign: 'center', py: 6 }}>
              <Storage sx={{ fontSize: 64, color: 'text.secondary', mb: 2 }} />
              <Typography variant="h6" gutterBottom>
                No buckets found
              </Typography>
              <Typography color="textSecondary" gutterBottom>
                Buckets are created automatically when you upload files.
              </Typography>
              <Button
                variant="contained"
                startIcon={<Folder />}
                onClick={() => navigate('/search')}
                sx={{ mt: 2 }}
              >
                Upload Files
              </Button>
            </Box>
          ) : (
            <List>
              {buckets.map((bucket, index) => {
                const stats = bucketStatsQueries.data?.[bucket];
                return (
                  <React.Fragment key={bucket}>
                    <ListItem
                      button
                      component={Link}
                      to={`/buckets/${bucket}`}
                      sx={{ 
                        borderRadius: 1,
                        mb: 1,
                        '&:hover': { 
                          backgroundColor: 'action.hover' 
                        }
                      }}
                    >
                      <Avatar sx={{ mr: 2, bgcolor: 'primary.main' }}>
                        <Folder />
                      </Avatar>
                      <ListItemText
                        primary={
                          <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                            <Typography variant="h6">{bucket}</Typography>
                            {stats?.compressed_files > 0 && (
                              <Chip label={`${stats.compressed_files} compressed`} size="small" color="primary" />
                            )}
                            {stats?.encrypted_files > 0 && (
                              <Chip label={`${stats.encrypted_files} encrypted`} size="small" color="secondary" />
                            )}
                          </Box>
                        }
                        secondary={
                          stats ? (
                            <Box>
                              <Typography variant="body2" color="textSecondary">
                                {stats.total_files.toLocaleString()} files • {formatBytes(stats.total_size)}
                              </Typography>
                              {stats.compression_ratio && (
                                <Typography variant="caption" color="textSecondary">
                                  Compression ratio: {(stats.compression_ratio * 100).toFixed(1)}%
                                </Typography>
                              )}
                            </Box>
                          ) : (
                            <Typography variant="body2" color="textSecondary">
                              Loading stats...
                            </Typography>
                          )
                        }
                      />
                      <ListItemSecondaryAction>
                        <IconButton
                          onClick={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            handleMenuClick(e, bucket);
                          }}
                          edge="end"
                        >
                          <MoreVert />
                        </IconButton>
                      </ListItemSecondaryAction>
                    </ListItem>
                    {index < buckets.length - 1 && <Divider />}
                  </React.Fragment>
                );
              })}
            </List>
          )}
        </CardContent>
      </Card>

      {/* Bucket Menu */}
      <Menu
        anchorEl={anchorEl}
        open={Boolean(anchorEl)}
        onClose={handleMenuClose}
      >
        <MenuItem onClick={() => menuBucket && handleView(menuBucket)}>
          <ListItemIcon><Visibility /></ListItemIcon>
          View Files
        </MenuItem>
        <MenuItem onClick={() => menuBucket && navigate(`/buckets/${menuBucket}`)}>
          <ListItemIcon><Analytics /></ListItemIcon>
          Bucket Stats
        </MenuItem>
        <Divider />
        <MenuItem 
          onClick={() => menuBucket && handleDelete(menuBucket)} 
          sx={{ color: 'error.main' }}
        >
          <ListItemIcon><Delete color="error" /></ListItemIcon>
          Delete Bucket
        </MenuItem>
      </Menu>

      {/* Delete Confirmation Dialog */}
      <Dialog open={deleteDialogOpen} onClose={() => setDeleteDialogOpen(false)}>
        <DialogTitle>Confirm Delete Bucket</DialogTitle>
        <DialogContent>
          <Typography>
            Are you sure you want to delete the bucket "{bucketToDelete}"? 
            This will permanently delete all files in this bucket. This action cannot be undone.
          </Typography>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setDeleteDialogOpen(false)}>Cancel</Button>
          <Button onClick={confirmDelete} color="error" disabled={deleteMutation.isPending}>
            {deleteMutation.isPending ? 'Deleting...' : 'Delete Bucket'}
          </Button>
        </DialogActions>
      </Dialog>
    </Box>
  );
} 