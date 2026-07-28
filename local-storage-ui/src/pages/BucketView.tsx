import React, { useState, useCallback, useMemo } from 'react';
import {
  Box, Grid, Card, CardContent, Typography, Button, IconButton, Dialog, DialogTitle, DialogContent, DialogActions,
  List, ListItem, ListItemText, ListItemSecondaryAction, Chip, TextField, Alert, LinearProgress,
  Menu, MenuItem, Divider, ListItemIcon, Breadcrumbs, Link, Paper, Tooltip, Fab,
  FormControlLabel, Switch, Collapse, Select, FormControl, InputLabel, Snackbar,
} from '@mui/material';
import {
  Download, Delete, Search, Folder, InsertDriveFile, CloudUpload, MoreVert,
  Info, Home, Refresh, NavigateNext, FolderOpen,
  AudioFile, Image, VideoFile, Description, Archive, Code,
  KeyboardArrowUp, KeyboardArrowDown, Sort, DeleteForever, Tune, ExpandMore, ExpandLess,
} from '@mui/icons-material';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useParams } from 'react-router-dom';
import { useDropzone } from 'react-dropzone';
import { listFiles, uploadFile, downloadFile, deleteFile, getBucketStats, getFileInfo, listEncryptionKeys } from '../api/client';
import { formatBytes } from '../utils/format';
import { buildFileTree, sortItems, breadcrumbs, FolderItem, FileEntry, SortBy, SortOrder } from '../utils/fileTree';
import { StoredFile, UploadOptions, EncryptionKeyInfo } from '../types/api';

export default function BucketView() {
  const { bucket } = useParams<{ bucket: string }>();
  const queryClient = useQueryClient();

  const [searchTerm, setSearchTerm] = useState('');
  const [currentPath, setCurrentPath] = useState('');
  const [sortBy, setSortBy] = useState<SortBy>('name');
  const [sortOrder, setSortOrder] = useState<SortOrder>('asc');
  const [showFoldersFirst, setShowFoldersFirst] = useState(true);

  const [selectedFile, setSelectedFile] = useState<StoredFile | null>(null);
  const [fileInfoOpen, setFileInfoOpen] = useState(false);
  const [fileToDelete, setFileToDelete] = useState<StoredFile | null>(null);
  const [folderToDelete, setFolderToDelete] = useState<FolderItem | null>(null);
  const [isDeletingFolder, setIsDeletingFolder] = useState(false);

  const [fileMenuAnchor, setFileMenuAnchor] = useState<null | HTMLElement>(null);
  const [menuFile, setMenuFile] = useState<StoredFile | null>(null);
  const [folderMenuAnchor, setFolderMenuAnchor] = useState<null | HTMLElement>(null);
  const [menuFolder, setMenuFolder] = useState<FolderItem | null>(null);

  const [uploadOptionsOpen, setUploadOptionsOpen] = useState(false);
  const [uploadOptions, setUploadOptions] = useState<UploadOptions>({});
  const [snackbar, setSnackbar] = useState<string | null>(null);

  const { data: files = [], isLoading: filesLoading, error: filesError, refetch } = useQuery<StoredFile[]>({
    queryKey: ['files', bucket],
    queryFn: () => listFiles({ bucket: bucket!, limit: 1000 }),
    enabled: !!bucket,
  });

  const { data: bucketStats } = useQuery({
    queryKey: ['bucket-stats', bucket],
    queryFn: () => getBucketStats(bucket!),
    enabled: !!bucket,
  });

  const { data: encryptionKeys = [] } = useQuery<EncryptionKeyInfo[]>({
    queryKey: ['encryption-keys'],
    queryFn: listEncryptionKeys,
    enabled: !!uploadOptions.encrypt,
  });

  const invalidateFiles = () => {
    queryClient.invalidateQueries({ queryKey: ['files', bucket] });
    queryClient.invalidateQueries({ queryKey: ['bucket-stats', bucket] });
  };

  const uploadMutation = useMutation({
    mutationFn: ({ file }: { file: File }) => {
      const key = currentPath ? `${currentPath}/${file.name}` : file.name;
      return uploadFile(bucket!, file, key, uploadOptions);
    },
    onSuccess: invalidateFiles,
    onError: () => setSnackbar('Upload failed'),
  });

  const deleteMutation = useMutation({
    mutationFn: ({ key }: { key: string }) => deleteFile(bucket!, key),
    onSuccess: () => {
      invalidateFiles();
      setFileToDelete(null);
    },
    onError: () => setSnackbar('Delete failed'),
  });

  const onDrop = useCallback((acceptedFiles: File[]) => {
    acceptedFiles.forEach((file) => uploadMutation.mutate({ file }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [uploadMutation, currentPath, uploadOptions]);

  const { getRootProps, getInputProps, isDragActive } = useDropzone({ onDrop });

  const { folders, files: fileEntries } = useMemo(() => buildFileTree(files, currentPath), [files, currentPath]);

  const filteredFolders = useMemo(
    () => folders.filter((f: FolderItem) => f.name.toLowerCase().includes(searchTerm.toLowerCase())),
    [folders, searchTerm],
  );
  const filteredFiles = useMemo(
    () => fileEntries.filter((f: FileEntry) =>
      f.name.toLowerCase().includes(searchTerm.toLowerCase())
      || f.file.content_type.toLowerCase().includes(searchTerm.toLowerCase())),
    [fileEntries, searchTerm],
  );

  const sortedFolders = useMemo(() => sortItems(filteredFolders, sortBy, sortOrder), [filteredFolders, sortBy, sortOrder]);
  const sortedFiles = useMemo(() => sortItems(filteredFiles, sortBy, sortOrder), [filteredFiles, sortBy, sortOrder]);

  const crumbs = useMemo(() => breadcrumbs(currentPath), [currentPath]);

  const handleDownload = async (file: StoredFile) => {
    try {
      const blob = await downloadFile(file.bucket, file.key);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = file.filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch {
      setSnackbar('Download failed');
    }
  };

  const handleFileInfo = async (file: StoredFile) => {
    try {
      setSelectedFile(await getFileInfo(file.bucket, file.key));
      setFileInfoOpen(true);
    } catch {
      setSnackbar('Failed to load file info');
    }
  };

  const confirmDeleteFolder = async () => {
    if (!folderToDelete || !bucket) return;
    setIsDeletingFolder(true);
    try {
      const toDelete = files.filter((f: StoredFile) => f.key.startsWith(folderToDelete.path + '/'));
      const batchSize = 5;
      for (let i = 0; i < toDelete.length; i += batchSize) {
        await Promise.all(toDelete.slice(i, i + batchSize).map((f: StoredFile) => deleteFile(bucket, f.key)));
      }
      invalidateFiles();
      setFolderToDelete(null);
    } catch {
      setSnackbar('Failed to delete folder');
    } finally {
      setIsDeletingFolder(false);
    }
  };

  const getFileIcon = (contentType: string) => {
    if (contentType.startsWith('image/')) return <Image color="success" />;
    if (contentType.startsWith('video/')) return <VideoFile color="error" />;
    if (contentType.startsWith('audio/')) return <AudioFile color="warning" />;
    if (contentType.includes('pdf')) return <Description color="info" />;
    if (contentType.includes('text') || contentType.includes('json') || contentType.includes('xml')) return <Code color="secondary" />;
    if (contentType.includes('zip') || contentType.includes('archive') || contentType.includes('tar')) return <Archive />;
    return <InsertDriveFile />;
  };

  if (!bucket) {
    return <Typography color="error" sx={{ p: 3 }}>No bucket specified</Typography>;
  }

  const itemCount = sortedFolders.length + sortedFiles.length;

  return (
    <Box sx={{ p: 3 }}>
      <Paper elevation={1} sx={{ p: 3, mb: 3, borderRadius: 2 }}>
        <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', mb: 2 }}>
          <Typography variant="h4" sx={{ display: 'flex', alignItems: 'center', gap: 1, fontWeight: 'bold' }}>
            <Folder color="primary" />
            {bucket}
          </Typography>
          <Box sx={{ display: 'flex', gap: 1 }}>
            <Tooltip title="Refresh">
              <span>
                <IconButton onClick={() => refetch()} disabled={filesLoading}>
                  <Refresh />
                </IconButton>
              </span>
            </Tooltip>
            <Tooltip title={sortOrder === 'asc' ? 'Sort ascending' : 'Sort descending'}>
              <IconButton onClick={() => setSortOrder(sortOrder === 'asc' ? 'desc' : 'asc')}>
                {sortOrder === 'asc' ? <KeyboardArrowUp /> : <KeyboardArrowDown />}
              </IconButton>
            </Tooltip>
          </Box>
        </Box>

        <Breadcrumbs separator={<NavigateNext fontSize="small" />} sx={{ mb: 2 }}>
          {crumbs.map((item, index) => (
            <Link
              key={item.path}
              color={index === crumbs.length - 1 ? 'text.primary' : 'inherit'}
              underline="hover"
              sx={{ cursor: 'pointer', display: 'flex', alignItems: 'center', gap: 0.5 }}
              onClick={() => setCurrentPath(item.path)}
            >
              {index === 0 && <Home fontSize="small" />}
              {item.name}
            </Link>
          ))}
        </Breadcrumbs>

        {bucketStats && (
          <Grid container spacing={2}>
            <Grid item xs={6} sm={3}>
              <Card sx={{ textAlign: 'center', bgcolor: 'primary.main', color: 'white' }}>
                <CardContent sx={{ py: 2 }}>
                  <Typography variant="h6">{bucketStats.total_files.toLocaleString()}</Typography>
                  <Typography variant="caption">Files</Typography>
                </CardContent>
              </Card>
            </Grid>
            <Grid item xs={6} sm={3}>
              <Card sx={{ textAlign: 'center', bgcolor: 'success.main', color: 'white' }}>
                <CardContent sx={{ py: 2 }}>
                  <Typography variant="h6">{formatBytes(bucketStats.total_size)}</Typography>
                  <Typography variant="caption">Total Size</Typography>
                </CardContent>
              </Card>
            </Grid>
            <Grid item xs={6} sm={3}>
              <Card sx={{ textAlign: 'center', bgcolor: 'warning.main', color: 'white' }}>
                <CardContent sx={{ py: 2 }}>
                  <Typography variant="h6">{bucketStats.compressed_files.toLocaleString()}</Typography>
                  <Typography variant="caption">Compressed</Typography>
                </CardContent>
              </Card>
            </Grid>
            <Grid item xs={6} sm={3}>
              <Card sx={{ textAlign: 'center', bgcolor: 'error.main', color: 'white' }}>
                <CardContent sx={{ py: 2 }}>
                  <Typography variant="h6">{bucketStats.encrypted_files.toLocaleString()}</Typography>
                  <Typography variant="caption">Encrypted</Typography>
                </CardContent>
              </Card>
            </Grid>
          </Grid>
        )}
      </Paper>

      <Paper elevation={1} sx={{ p: 2, mb: 3, borderRadius: 2 }}>
        <Grid container spacing={2} alignItems="center">
          <Grid item xs={12} md={6}>
            <TextField
              fullWidth
              variant="outlined"
              placeholder="Filter files and folders..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              InputProps={{ startAdornment: <Search sx={{ mr: 1, color: 'text.secondary' }} /> }}
              size="small"
            />
          </Grid>
          <Grid item xs={12} md={6}>
            <Box sx={{ display: 'flex', gap: 1, justifyContent: 'flex-end' }}>
              <Button
                variant={showFoldersFirst ? 'contained' : 'outlined'}
                size="small"
                startIcon={<Folder />}
                onClick={() => setShowFoldersFirst(!showFoldersFirst)}
              >
                Folders First
              </Button>
              <Button
                variant="outlined"
                size="small"
                startIcon={<Sort />}
                onClick={() => {
                  const options: SortBy[] = ['name', 'size', 'date'];
                  setSortBy(options[(options.indexOf(sortBy) + 1) % options.length]);
                }}
              >
                Sort: {sortBy}
              </Button>
            </Box>
          </Grid>
        </Grid>
      </Paper>

      <Paper
        elevation={isDragActive ? 8 : 1}
        sx={{
          mb: 1, borderRadius: 2,
          border: isDragActive ? '3px dashed #1976d2' : '2px dashed #e0e0e0',
          transition: 'all 0.2s ease',
        }}
      >
        <CardContent>
          <Box {...getRootProps()} sx={{ textAlign: 'center', py: 6, cursor: 'pointer' }}>
            <input {...getInputProps()} />
            <CloudUpload sx={{ fontSize: 64, color: 'primary.main', mb: 2 }} />
            <Typography variant="h5" gutterBottom color="primary">
              {isDragActive ? 'Drop files here' : 'Drag & drop files here, or click to select'}
            </Typography>
            <Typography color="textSecondary" variant="body1">
              Upload files to {currentPath ? `/${currentPath}` : 'the bucket root'}
            </Typography>
          </Box>
        </CardContent>
      </Paper>

      <Box sx={{ mb: 3 }}>
        <Button
          size="small"
          startIcon={<Tune />}
          endIcon={uploadOptionsOpen ? <ExpandLess /> : <ExpandMore />}
          onClick={() => setUploadOptionsOpen(!uploadOptionsOpen)}
        >
          Upload options {uploadOptions.compress === false || uploadOptions.encrypt ? '(customized)' : ''}
        </Button>
        <Collapse in={uploadOptionsOpen}>
          <Paper sx={{ p: 2, mt: 1, borderRadius: 2 }}>
            <Grid container spacing={2} alignItems="center">
              <Grid item>
                <FormControlLabel
                  control={
                    <Switch
                      checked={uploadOptions.compress !== false}
                      onChange={(e) => setUploadOptions((o) => ({ ...o, compress: e.target.checked }))}
                    />
                  }
                  label="Compress (server default if unset)"
                />
              </Grid>
              <Grid item>
                <FormControlLabel
                  control={
                    <Switch
                      checked={!!uploadOptions.encrypt}
                      onChange={(e) => setUploadOptions((o) => ({ ...o, encrypt: e.target.checked || undefined }))}
                    />
                  }
                  label="Encrypt"
                />
              </Grid>
              {uploadOptions.encrypt && (
                <Grid item xs={12} sm={4}>
                  <FormControl fullWidth size="small">
                    <InputLabel>Encryption key</InputLabel>
                    <Select
                      label="Encryption key"
                      value={uploadOptions.encryption_key_id ?? ''}
                      onChange={(e) => setUploadOptions((o) => ({ ...o, encryption_key_id: e.target.value || undefined }))}
                    >
                      <MenuItem value="">Server default key</MenuItem>
                      {encryptionKeys.map((k: EncryptionKeyInfo) => (
                        <MenuItem key={k.key_id} value={k.key_id}>
                          {k.description || k.key_id.slice(0, 12)}
                        </MenuItem>
                      ))}
                    </Select>
                  </FormControl>
                </Grid>
              )}
            </Grid>
          </Paper>
        </Collapse>
      </Box>

      {uploadMutation.isPending && (
        <Alert severity="info" sx={{ mb: 2, borderRadius: 2 }}>
          <LinearProgress sx={{ mt: 1 }} />
          Uploading file...
        </Alert>
      )}
      {filesError instanceof Error && (
        <Alert severity="error" sx={{ mb: 2, borderRadius: 2 }}>Failed to load files: {filesError.message}</Alert>
      )}

      <Paper elevation={1} sx={{ borderRadius: 2 }}>
        <CardContent>
          <Typography variant="h6" sx={{ mb: 2, display: 'flex', alignItems: 'center', gap: 1 }}>
            <InsertDriveFile />
            {currentPath ? `Contents of /${currentPath}` : 'Root Directory'}
            <Chip label={`${itemCount} items`} size="small" color="primary" variant="outlined" />
          </Typography>

          {filesLoading ? (
            <LinearProgress />
          ) : itemCount === 0 ? (
            <Box sx={{ textAlign: 'center', py: 8 }}>
              <Folder sx={{ fontSize: 64, color: 'text.secondary', mb: 2 }} />
              <Typography color="textSecondary" variant="h6" gutterBottom>
                {searchTerm ? 'No files match your search' : 'This folder is empty'}
              </Typography>
            </Box>
          ) : (
            <List>
              {(showFoldersFirst ? [...sortedFolders, ...sortedFiles] : [...sortedFiles, ...sortedFolders]).map((item, index, arr) => (
                <React.Fragment key={item.path}>
                  {item.type === 'folder' ? (
                    <ListItem sx={{ borderRadius: 1, mb: 0.5, '&:hover': { backgroundColor: 'action.hover' } }}>
                      <ListItemIcon onClick={() => setCurrentPath(item.path)} sx={{ cursor: 'pointer' }}>
                        <FolderOpen color="primary" />
                      </ListItemIcon>
                      <ListItemText
                        onClick={() => setCurrentPath(item.path)}
                        sx={{ cursor: 'pointer' }}
                        primaryTypographyProps={{ component: 'div' }}
                        primary={
                          <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, flexWrap: 'wrap' }}>
                            <Typography variant="subtitle1" fontWeight="medium">{item.name}</Typography>
                            <Chip label={`${item.fileCount} files`} size="small" color="primary" variant="outlined" />
                            <Chip label={formatBytes(item.totalSize)} size="small" color="secondary" variant="outlined" />
                          </Box>
                        }
                      />
                      <ListItemSecondaryAction>
                        <IconButton
                          onClick={(e) => { setFolderMenuAnchor(e.currentTarget); setMenuFolder(item); }}
                          edge="end"
                        >
                          <MoreVert />
                        </IconButton>
                      </ListItemSecondaryAction>
                    </ListItem>
                  ) : (
                    <ListItem sx={{ borderRadius: 1, mb: 0.5, '&:hover': { backgroundColor: 'action.hover' } }}>
                      <ListItemIcon>{getFileIcon(item.file.content_type)}</ListItemIcon>
                      <ListItemText
                        primaryTypographyProps={{ component: 'div' }}
                        secondaryTypographyProps={{ component: 'div' }}
                        primary={
                          <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, flexWrap: 'wrap' }}>
                            <Typography variant="subtitle1" fontWeight="medium">{item.name}</Typography>
                            <Chip label={formatBytes(item.file.file_size)} size="small" color="info" variant="outlined" />
                            {item.file.is_compressed && <Chip label="Compressed" size="small" color="primary" />}
                            {item.file.is_encrypted && <Chip label="Encrypted" size="small" color="secondary" />}
                          </Box>
                        }
                        secondary={
                          <Box>
                            <Typography variant="body2" color="textSecondary">{item.file.content_type}</Typography>
                            <Typography variant="caption" color="textSecondary">
                              Uploaded: {new Date(item.file.upload_time).toLocaleDateString()} • Access count: {item.file.access_count}
                            </Typography>
                          </Box>
                        }
                      />
                      <ListItemSecondaryAction>
                        <IconButton onClick={(e) => { setFileMenuAnchor(e.currentTarget); setMenuFile(item.file); }} edge="end">
                          <MoreVert />
                        </IconButton>
                      </ListItemSecondaryAction>
                    </ListItem>
                  )}
                  {index < arr.length - 1 && <Divider />}
                </React.Fragment>
              ))}
            </List>
          )}
        </CardContent>
      </Paper>

      <Fab color="primary" aria-label="upload" sx={{ position: 'fixed', bottom: 16, right: 16 }} {...getRootProps()}>
        <input {...getInputProps()} />
        <CloudUpload />
      </Fab>

      <Menu anchorEl={fileMenuAnchor} open={Boolean(fileMenuAnchor)} onClose={() => setFileMenuAnchor(null)}>
        <MenuItem onClick={() => { menuFile && handleDownload(menuFile); setFileMenuAnchor(null); }}>
          <ListItemIcon><Download /></ListItemIcon>
          Download
        </MenuItem>
        <MenuItem onClick={() => { menuFile && handleFileInfo(menuFile); setFileMenuAnchor(null); }}>
          <ListItemIcon><Info /></ListItemIcon>
          File Info
        </MenuItem>
        <Divider />
        <MenuItem
          onClick={() => { setFileToDelete(menuFile); setFileMenuAnchor(null); }}
          sx={{ color: 'error.main' }}
        >
          <ListItemIcon><Delete color="error" /></ListItemIcon>
          Delete
        </MenuItem>
      </Menu>

      <Menu anchorEl={folderMenuAnchor} open={Boolean(folderMenuAnchor)} onClose={() => setFolderMenuAnchor(null)}>
        <MenuItem onClick={() => { menuFolder && setCurrentPath(menuFolder.path); setFolderMenuAnchor(null); }}>
          <ListItemIcon><FolderOpen /></ListItemIcon>
          Open Folder
        </MenuItem>
        <Divider />
        <MenuItem
          onClick={() => { setFolderToDelete(menuFolder); setFolderMenuAnchor(null); }}
          sx={{ color: 'error.main' }}
        >
          <ListItemIcon><DeleteForever color="error" /></ListItemIcon>
          Delete Folder & All Files
        </MenuItem>
      </Menu>

      <Dialog open={fileInfoOpen} onClose={() => setFileInfoOpen(false)} maxWidth="md" fullWidth>
        <DialogTitle>File Information</DialogTitle>
        <DialogContent>
          {selectedFile && (
            <Grid container spacing={2}>
              <Grid item xs={12} sm={6}>
                <Typography variant="subtitle2" color="textSecondary">Filename</Typography>
                <Typography variant="body1" gutterBottom>{selectedFile.filename}</Typography>
              </Grid>
              <Grid item xs={12} sm={6}>
                <Typography variant="subtitle2" color="textSecondary">Size</Typography>
                <Typography variant="body1" gutterBottom>
                  {formatBytes(selectedFile.file_size)}
                  {selectedFile.is_compressed && ` (from ${formatBytes(selectedFile.original_size)})`}
                </Typography>
              </Grid>
              <Grid item xs={12} sm={6}>
                <Typography variant="subtitle2" color="textSecondary">Content Type</Typography>
                <Typography variant="body1" gutterBottom>{selectedFile.content_type}</Typography>
              </Grid>
              <Grid item xs={12} sm={6}>
                <Typography variant="subtitle2" color="textSecondary">Upload Date</Typography>
                <Typography variant="body1" gutterBottom>{new Date(selectedFile.upload_time).toLocaleString()}</Typography>
              </Grid>
              <Grid item xs={12} sm={6}>
                <Typography variant="subtitle2" color="textSecondary">Last Accessed</Typography>
                <Typography variant="body1" gutterBottom>
                  {selectedFile.last_accessed ? new Date(selectedFile.last_accessed).toLocaleString() : 'Never'}
                </Typography>
              </Grid>
              <Grid item xs={12} sm={6}>
                <Typography variant="subtitle2" color="textSecondary">Access Count</Typography>
                <Typography variant="body1" gutterBottom>{selectedFile.access_count}</Typography>
              </Grid>
              <Grid item xs={12} sm={6}>
                <Typography variant="subtitle2" color="textSecondary">BLAKE3 Hash</Typography>
                <Typography variant="body2" sx={{ fontFamily: 'monospace', wordBreak: 'break-all' }} gutterBottom>
                  {selectedFile.hash_blake3}
                </Typography>
              </Grid>
              <Grid item xs={12} sm={6}>
                <Typography variant="subtitle2" color="textSecondary">MD5 Hash</Typography>
                <Typography variant="body2" sx={{ fontFamily: 'monospace', wordBreak: 'break-all' }} gutterBottom>
                  {selectedFile.hash_md5}
                </Typography>
              </Grid>
              {selectedFile.is_compressed && (
                <Grid item xs={12} sm={6}>
                  <Typography variant="subtitle2" color="textSecondary">Compression</Typography>
                  <Typography variant="body1" gutterBottom>
                    {selectedFile.compression_algorithm} • Ratio: {selectedFile.compression_ratio?.toFixed(2)}
                  </Typography>
                </Grid>
              )}
              {selectedFile.is_encrypted && (
                <Grid item xs={12} sm={6}>
                  <Typography variant="subtitle2" color="textSecondary">Encryption</Typography>
                  <Typography variant="body1" gutterBottom>
                    {selectedFile.encryption_algorithm}
                    {selectedFile.encryption_key_id && ` • Key: ${selectedFile.encryption_key_id.slice(0, 12)}...`}
                  </Typography>
                </Grid>
              )}
            </Grid>
          )}
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setFileInfoOpen(false)}>Close</Button>
        </DialogActions>
      </Dialog>

      <Dialog open={!!fileToDelete} onClose={() => setFileToDelete(null)}>
        <DialogTitle>Confirm Delete</DialogTitle>
        <DialogContent>
          <Typography>
            Are you sure you want to delete "{fileToDelete?.filename}"? This action cannot be undone.
          </Typography>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setFileToDelete(null)}>Cancel</Button>
          <Button
            onClick={() => fileToDelete && deleteMutation.mutate({ key: fileToDelete.key })}
            color="error"
            disabled={deleteMutation.isPending}
          >
            {deleteMutation.isPending ? 'Deleting...' : 'Delete'}
          </Button>
        </DialogActions>
      </Dialog>

      <Dialog open={!!folderToDelete} onClose={() => !isDeletingFolder && setFolderToDelete(null)}>
        <DialogTitle>Confirm Delete Folder</DialogTitle>
        <DialogContent>
          {!isDeletingFolder ? (
            <>
              <Alert severity="warning" sx={{ mb: 2 }}>This will permanently delete the folder and all its contents!</Alert>
              <Typography gutterBottom>
                Are you sure you want to delete the folder <strong>"{folderToDelete?.name}"</strong>?
              </Typography>
              <Typography variant="body2" color="textSecondary" gutterBottom>
                • Contains <strong>{folderToDelete?.fileCount ?? 0} files</strong>, total <strong>{formatBytes(folderToDelete?.totalSize ?? 0)}</strong>
              </Typography>
            </>
          ) : (
            <>
              <Alert severity="info" sx={{ mb: 2 }}>Deleting folder and all files... Please wait.</Alert>
              <LinearProgress sx={{ mt: 2 }} />
            </>
          )}
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setFolderToDelete(null)} disabled={isDeletingFolder}>Cancel</Button>
          <Button onClick={confirmDeleteFolder} color="error" variant="contained" disabled={isDeletingFolder}>
            {isDeletingFolder ? 'Deleting...' : 'Delete Folder & All Files'}
          </Button>
        </DialogActions>
      </Dialog>

      <Snackbar open={!!snackbar} autoHideDuration={4000} onClose={() => setSnackbar(null)} message={snackbar} />
    </Box>
  );
}
