import React, { useState } from 'react';
import {
  Box, Card, CardContent, Typography, Grid, IconButton, Dialog, DialogTitle, DialogContent,
  DialogActions, Button, List, ListItem, ListItemText, ListItemSecondaryAction, Alert,
  Menu, MenuItem, Divider, ListItemIcon, Avatar, Chip, TextField,
} from '@mui/material';
import {
  Folder, MoreVert, Delete, Storage, Visibility, Analytics, Add,
} from '@mui/icons-material';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate } from 'react-router-dom';
import { listBuckets, deleteBucket, createBucket, getBucketStats } from '../api/client';
import { formatBytes } from '../utils/format';
import { BucketStats } from '../types/api';

export default function Buckets() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [bucketToDelete, setBucketToDelete] = useState<string | null>(null);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [newBucketName, setNewBucketName] = useState('');
  const [anchorEl, setAnchorEl] = useState<null | HTMLElement>(null);
  const [menuBucket, setMenuBucket] = useState<string | null>(null);

  const { data: buckets = [], isLoading, error } = useQuery<string[]>({
    queryKey: ['buckets'],
    queryFn: listBuckets,
  });

  // One stats request per bucket, merged into a single { [bucket]: BucketStats } map.
  const bucketStatsQueries = useQuery({
    queryKey: ['all-bucket-stats', buckets],
    queryFn: async (): Promise<Record<string, BucketStats>> => {
      const entries = await Promise.all(
        buckets.map(async (bucket: string): Promise<[string, BucketStats | null]> => {
          try {
            return [bucket, await getBucketStats(bucket)];
          } catch {
            return [bucket, null];
          }
        }),
      );
      return Object.fromEntries(entries.filter((e): e is [string, BucketStats] => e[1] !== null));
    },
    enabled: buckets.length > 0,
  });

  const createMutation = useMutation({
    mutationFn: createBucket,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['buckets'] });
      setCreateDialogOpen(false);
      setNewBucketName('');
    },
  });

  const deleteMutation = useMutation({
    mutationFn: deleteBucket,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['buckets'] });
      queryClient.invalidateQueries({ queryKey: ['all-bucket-stats'] });
      setDeleteDialogOpen(false);
      setBucketToDelete(null);
    },
  });

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

  const statsValues: BucketStats[] = Object.values(bucketStatsQueries.data ?? {});
  const totalFiles = statsValues.reduce((acc, s) => acc + s.total_files, 0);
  const totalSize = statsValues.reduce((acc, s) => acc + s.total_size, 0);
  const totalCompressed = statsValues.reduce((acc, s) => acc + s.compressed_files, 0);

  return (
    <Box sx={{ p: 3 }}>
      <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', mb: 2 }}>
        <Typography variant="h4" sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
          <Storage color="primary" />
          Buckets ({buckets.length})
        </Typography>
        <Button variant="contained" startIcon={<Add />} onClick={() => setCreateDialogOpen(true)}>
          New Bucket
        </Button>
      </Box>

      <Grid container spacing={2} sx={{ mb: 3 }}>
        <Grid item xs={12} sm={6} md={4}>
          <Card>
            <CardContent sx={{ textAlign: 'center' }}>
              <Typography color="textSecondary" gutterBottom>Total Files</Typography>
              <Typography variant="h4">{totalFiles.toLocaleString()}</Typography>
            </CardContent>
          </Card>
        </Grid>
        <Grid item xs={12} sm={6} md={4}>
          <Card>
            <CardContent sx={{ textAlign: 'center' }}>
              <Typography color="textSecondary" gutterBottom>Total Size</Typography>
              <Typography variant="h4">{formatBytes(totalSize)}</Typography>
            </CardContent>
          </Card>
        </Grid>
        <Grid item xs={12} sm={6} md={4}>
          <Card>
            <CardContent sx={{ textAlign: 'center' }}>
              <Typography color="textSecondary" gutterBottom>Compressed Files</Typography>
              <Typography variant="h4">{totalCompressed.toLocaleString()}</Typography>
            </CardContent>
          </Card>
        </Grid>
      </Grid>

      {error instanceof Error && (
        <Alert severity="error" sx={{ mb: 2 }}>Failed to load buckets: {error.message}</Alert>
      )}
      {deleteMutation.error instanceof Error && (
        <Alert severity="error" sx={{ mb: 2 }}>Failed to delete bucket: {deleteMutation.error.message}</Alert>
      )}
      {createMutation.error instanceof Error && (
        <Alert severity="error" sx={{ mb: 2 }}>Failed to create bucket: {createMutation.error.message}</Alert>
      )}

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
              <Typography variant="h6" gutterBottom>No buckets found</Typography>
              <Typography color="textSecondary" gutterBottom>
                Create a bucket to start storing files.
              </Typography>
              <Button variant="contained" startIcon={<Add />} onClick={() => setCreateDialogOpen(true)} sx={{ mt: 2 }}>
                New Bucket
              </Button>
            </Box>
          ) : (
            <List>
              {buckets.map((bucket: string, index: number) => {
                const stats = bucketStatsQueries.data?.[bucket];
                return (
                  <React.Fragment key={bucket}>
                    <ListItem
                      component={Link}
                      to={`/buckets/${bucket}`}
                      sx={{ borderRadius: 1, mb: 1, '&:hover': { backgroundColor: 'action.hover' } }}
                    >
                      <Avatar sx={{ mr: 2, bgcolor: 'primary.main' }}>
                        <Folder />
                      </Avatar>
                      <ListItemText
                        primaryTypographyProps={{ component: 'div' }}
                        secondaryTypographyProps={{ component: 'div' }}
                        primary={
                          <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                            <Typography variant="h6">{bucket}</Typography>
                            {!!stats?.compressed_files && (
                              <Chip label={`${stats.compressed_files} compressed`} size="small" color="primary" />
                            )}
                            {!!stats?.encrypted_files && (
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
                              {!!stats.compression_ratio && (
                                <Typography variant="caption" color="textSecondary">
                                  Compression ratio: {(stats.compression_ratio * 100).toFixed(1)}%
                                </Typography>
                              )}
                            </Box>
                          ) : (
                            <Typography variant="body2" color="textSecondary">Loading stats...</Typography>
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

      <Menu anchorEl={anchorEl} open={Boolean(anchorEl)} onClose={handleMenuClose}>
        <MenuItem onClick={() => menuBucket && handleView(menuBucket)}>
          <ListItemIcon><Visibility /></ListItemIcon>
          View Files
        </MenuItem>
        <MenuItem onClick={() => menuBucket && navigate(`/buckets/${menuBucket}`)}>
          <ListItemIcon><Analytics /></ListItemIcon>
          Bucket Stats
        </MenuItem>
        <Divider />
        <MenuItem onClick={() => menuBucket && handleDelete(menuBucket)} sx={{ color: 'error.main' }}>
          <ListItemIcon><Delete color="error" /></ListItemIcon>
          Delete Bucket
        </MenuItem>
      </Menu>

      <Dialog open={createDialogOpen} onClose={() => setCreateDialogOpen(false)}>
        <DialogTitle>New Bucket</DialogTitle>
        <DialogContent>
          <TextField
            autoFocus
            fullWidth
            margin="dense"
            label="Bucket name"
            value={newBucketName}
            onChange={(e) => setNewBucketName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && newBucketName.trim() && createMutation.mutate(newBucketName.trim())}
          />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setCreateDialogOpen(false)}>Cancel</Button>
          <Button
            variant="contained"
            disabled={!newBucketName.trim() || createMutation.isPending}
            onClick={() => createMutation.mutate(newBucketName.trim())}
          >
            {createMutation.isPending ? 'Creating...' : 'Create'}
          </Button>
        </DialogActions>
      </Dialog>

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
          <Button
            onClick={() => bucketToDelete && deleteMutation.mutate(bucketToDelete)}
            color="error"
            disabled={deleteMutation.isPending}
          >
            {deleteMutation.isPending ? 'Deleting...' : 'Delete'}
          </Button>
        </DialogActions>
      </Dialog>
    </Box>
  );
}
