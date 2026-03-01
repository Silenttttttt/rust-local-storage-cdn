import React, { useState, useCallback, useMemo } from 'react';
import {
  Box, Grid, Card, CardContent, Typography, Button, IconButton, Dialog, DialogTitle, DialogContent, DialogActions,
  List, ListItem, ListItemText, ListItemSecondaryAction, Chip, TextField, Alert, LinearProgress,
  Menu, MenuItem, Divider, ListItemIcon, Breadcrumbs, Link, Paper, Tooltip,
  Fab
} from '@mui/material';
import {
  Download, Delete, Search, Folder, InsertDriveFile, CloudUpload, MoreVert,
  Info, Home, Refresh, NavigateNext, FolderOpen, 
  AudioFile, Image, VideoFile, Description, Archive, Code, 
  KeyboardArrowUp, KeyboardArrowDown, Sort, DeleteForever
} from '@mui/icons-material';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useParams } from 'react-router-dom';
import { useDropzone } from 'react-dropzone';
import { listFiles, uploadFile, downloadFile, deleteFile, getBucketStats, getFileInfo } from '../api/client';
import { formatBytes } from '../utils/format';
import { StoredFile } from '../types/api';

interface FileItem {
  type: 'file' | 'folder';
  name: string;
  displayName: string; // For collapsed paths like "folder1/folder2/folder3"
  path: string;
  file?: StoredFile;
  size?: number;
  fileCount?: number; // Total files in this folder (recursive)
  children?: FileItem[];
}

export default function BucketView() {
  const { bucket } = useParams<{ bucket: string }>();
  const queryClient = useQueryClient();
  const [searchTerm, setSearchTerm] = useState('');
  const [currentPath, setCurrentPath] = useState('');
  const [selectedFile, setSelectedFile] = useState<StoredFile | null>(null);
  const [fileInfoOpen, setFileInfoOpen] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [fileToDelete, setFileToDelete] = useState<StoredFile | null>(null);
  const [anchorEl, setAnchorEl] = useState<null | HTMLElement>(null);
  const [menuFile, setMenuFile] = useState<StoredFile | null>(null);
  const [sortBy, setSortBy] = useState<'name' | 'size' | 'date'>('name');
  const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>('asc');
  const [showFoldersFirst, setShowFoldersFirst] = useState(true);
  const [folderToDelete, setFolderToDelete] = useState<FileItem | null>(null);
  const [deleteFolderDialogOpen, setDeleteFolderDialogOpen] = useState(false);
  const [folderMenuAnchorEl, setFolderMenuAnchorEl] = useState<null | HTMLElement>(null);
  const [menuFolder, setMenuFolder] = useState<FileItem | null>(null);
  const [isDeletingFolder, setIsDeletingFolder] = useState(false);

  // Queries
  const { data: files = [], isLoading: filesLoading, error: filesError, refetch } = useQuery({
    queryKey: ['files', bucket, currentPath],
    queryFn: () => listFiles({ bucket: bucket!, prefix: currentPath }),
    enabled: !!bucket,
  });

  const { data: bucketStats } = useQuery({
    queryKey: ['bucket-stats', bucket],
    queryFn: () => getBucketStats(bucket!),
    enabled: !!bucket,
  });

  // Mutations
  const uploadMutation = useMutation({
    mutationFn: ({ file }: { file: File }) => {
      const uploadPath = currentPath ? `${currentPath}/${file.name}` : file.name;
      return uploadFile(bucket!, file, uploadPath);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['files', bucket, currentPath] });
      queryClient.invalidateQueries({ queryKey: ['bucket-stats', bucket] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: ({ key }: { key: string }) => deleteFile(bucket!, key),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['files', bucket, currentPath] });
      queryClient.invalidateQueries({ queryKey: ['bucket-stats', bucket] });
      setDeleteDialogOpen(false);
      setFileToDelete(null);
    },
  });

  // File drop zone
  const onDrop = useCallback((acceptedFiles: File[]) => {
    acceptedFiles.forEach(file => {
      uploadMutation.mutate({ file });
    });
  }, [uploadMutation]);

  const { getRootProps, getInputProps, isDragActive } = useDropzone({ onDrop });

  // Helper function to calculate folder stats recursively
  const calculateFolderStats = (folderPath: string, allFiles: StoredFile[]) => {
    let totalSize = 0;
    let fileCount = 0;
    
    allFiles.forEach(file => {
      if (file.key.startsWith(folderPath + '/')) {
        totalSize += file.file_size;
        fileCount++;
      }
    });
    
    return { totalSize, fileCount };
  };

  // Helper function to collapse single-child folders
  const collapseFolders = (folderPath: string, folderName: string, allFiles: StoredFile[]): { displayName: string, finalPath: string } => {
    let currentPath = folderPath;
    let displayParts = [folderName];
    
    while (true) {
      // Get immediate children of current folder
      const prefix = currentPath + '/';
      const children = allFiles.filter(f => {
        const relativePath = f.key.substring(prefix.length);
        return f.key.startsWith(prefix) && relativePath.length > 0;
      });
      
      if (children.length === 0) break;
      
      // Get unique first-level subfolders
      const subfolders = new Set<string>();
      const directFiles = children.filter(f => {
        const relativePath = f.key.substring(prefix.length);
        const parts = relativePath.split('/');
        if (parts.length > 1) {
          subfolders.add(parts[0]);
          return false;
        }
        return true;
      });
      
      // Only collapse if there's exactly 1 subfolder and no files at this level
      if (subfolders.size === 1 && directFiles.length === 0) {
        const subfolderName = Array.from(subfolders)[0];
        displayParts.push(subfolderName);
        currentPath = `${currentPath}/${subfolderName}`;
      } else {
        break;
      }
    }
    
    return {
      displayName: displayParts.join('/'),
      finalPath: currentPath
    };
  };

  // Organize files into folders and files
  const organizedItems = useMemo(() => {
    const folders = new Map<string, FileItem>();
    const fileItems: FileItem[] = [];

    files.forEach(file => {
      const relativePath = currentPath ? file.key.replace(currentPath + '/', '') : file.key;
      const pathParts = relativePath.split('/');
      
      if (pathParts.length > 1) {
        // This is a file in a subfolder
        const folderName = pathParts[0];
        const folderPath = currentPath ? `${currentPath}/${folderName}` : folderName;
        
        if (!folders.has(folderPath)) {
          const { displayName, finalPath } = collapseFolders(folderPath, folderName, files);
          const stats = calculateFolderStats(folderPath, files);
          
          folders.set(folderPath, {
            type: 'folder',
            name: folderName,
            displayName: displayName,
            path: finalPath, // Navigate to the deepest collapsed folder
            size: stats.totalSize,
            fileCount: stats.fileCount,
            children: []
          });
        }
      } else {
        // This is a file in the current directory
        const displayName = file.filename.includes('/') ? file.filename.split('/').pop()! : file.filename;
        fileItems.push({
          type: 'file',
          name: displayName,
          displayName: displayName,
          path: file.key,
          file: file,
          size: file.file_size
        });
      }
    });

    const folderItems = Array.from(folders.values());
    
    // Sort items
    const sortItems = (items: FileItem[]) => {
      return items.sort((a, b) => {
        if (showFoldersFirst && a.type !== b.type) {
          return a.type === 'folder' ? -1 : 1;
        }
        
        let comparison = 0;
        switch (sortBy) {
          case 'name':
            comparison = a.displayName.localeCompare(b.displayName);
            break;
          case 'size':
            comparison = (a.size || 0) - (b.size || 0);
            break;
          case 'date':
            comparison = new Date(a.file?.upload_time || 0).getTime() - new Date(b.file?.upload_time || 0).getTime();
            break;
        }
        
        return sortOrder === 'asc' ? comparison : -comparison;
      });
    };

    return {
      folders: sortItems(folderItems),
      files: sortItems(fileItems)
    };
  }, [files, currentPath, sortBy, sortOrder, showFoldersFirst]);

  // Filter items based on search
  const filteredItems = useMemo(() => {
    if (!searchTerm) return organizedItems;
    
    const filterItems = (items: FileItem[]): FileItem[] => {
      return items.filter(item => 
        item.displayName.toLowerCase().includes(searchTerm.toLowerCase()) ||
        item.file?.content_type.toLowerCase().includes(searchTerm.toLowerCase())
      );
    };

    return {
      folders: filterItems(organizedItems.folders),
      files: filterItems(organizedItems.files)
    };
  }, [organizedItems, searchTerm]);

  // Breadcrumb navigation
  const breadcrumbItems = useMemo(() => {
    const parts = currentPath.split('/').filter(Boolean);
    return [
      { name: 'Home', path: '' },
      ...parts.map((part, index) => ({
        name: part,
        path: parts.slice(0, index + 1).join('/')
      }))
    ];
  }, [currentPath]);

  // Handlers
  const handleDownload = async (file: StoredFile) => {
    try {
      const blob = await downloadFile(file.bucket, file.key);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      // Use the filename, but if it contains a path, extract just the filename
      const downloadName = file.filename.includes('/') ? file.filename.split('/').pop()! : file.filename;
      a.download = downloadName;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (error) {
      console.error('Download failed:', error);
    }
  };

  const handleDelete = (file: StoredFile) => {
    setFileToDelete(file);
    setDeleteDialogOpen(true);
    setAnchorEl(null);
  };

  const handleFileInfo = async (file: StoredFile) => {
    try {
      const fileInfo = await getFileInfo(file.bucket, file.key);
      setSelectedFile(fileInfo);
      setFileInfoOpen(true);
    } catch (error) {
      console.error('Failed to get file info:', error);
    }
  };

  const handleMenuClick = (event: React.MouseEvent<HTMLElement>, file: StoredFile) => {
    setAnchorEl(event.currentTarget);
    setMenuFile(file);
  };

  const handleMenuClose = () => {
    setAnchorEl(null);
    setMenuFile(null);
  };

  const confirmDelete = () => {
    if (fileToDelete) {
      deleteMutation.mutate({ key: fileToDelete.key });
    }
  };

  const handleFolderMenuClick = (event: React.MouseEvent<HTMLElement>, folder: FileItem) => {
    setFolderMenuAnchorEl(event.currentTarget);
    setMenuFolder(folder);
  };

  const handleFolderMenuClose = () => {
    setFolderMenuAnchorEl(null);
    setMenuFolder(null);
  };

  const handleDeleteFolder = (folder: FileItem) => {
    setFolderToDelete(folder);
    setDeleteFolderDialogOpen(true);
    setFolderMenuAnchorEl(null);
  };

  const confirmDeleteFolder = async () => {
    if (!folderToDelete) return;
    
    setIsDeletingFolder(true);
    
    try {
      // Get all files in this folder
      const folderPrefix = folderToDelete.name; // Use the base folder name
      const basePath = currentPath ? `${currentPath}/${folderPrefix}` : folderPrefix;
      
      // Delete all files in the folder
      const filesToDelete = files.filter(f => f.key.startsWith(basePath + '/'));
      
      console.log(`Deleting ${filesToDelete.length} files from folder: ${basePath}`);
      
      // Delete files in parallel batches for better performance
      const batchSize = 5;
      for (let i = 0; i < filesToDelete.length; i += batchSize) {
        const batch = filesToDelete.slice(i, i + batchSize);
        await Promise.all(batch.map(file => deleteFile(bucket!, file.key)));
      }
      
      // Refresh the file list
      queryClient.invalidateQueries({ queryKey: ['files', bucket, currentPath] });
      queryClient.invalidateQueries({ queryKey: ['bucket-stats', bucket] });
      
      setDeleteFolderDialogOpen(false);
      setFolderToDelete(null);
    } catch (error) {
      console.error('Failed to delete folder:', error);
      alert('Failed to delete folder. Please try again.');
    } finally {
      setIsDeletingFolder(false);
    }
  };

  const navigateToFolder = (path: string) => {
    setCurrentPath(path);
  };

  const getFileIcon = (contentType: string, isFolder: boolean = false) => {
    if (isFolder) return <FolderOpen color="primary" />;
    
    if (contentType.startsWith('image/')) return <Image color="success" />;
    if (contentType.startsWith('video/')) return <VideoFile color="error" />;
    if (contentType.startsWith('audio/')) return <AudioFile color="warning" />;
    if (contentType.includes('pdf')) return <Description color="info" />;
    if (contentType.includes('text') || contentType.includes('json') || contentType.includes('xml')) return <Code color="secondary" />;
    if (contentType.includes('zip') || contentType.includes('archive') || contentType.includes('tar')) return <Archive />;
    return <InsertDriveFile />;
  };

  if (!bucket) {
    return <Typography color="error">No bucket specified</Typography>;
  }

  return (
    <Box sx={{ p: 3, minHeight: '100vh' }}>
      {/* Header */}
      <Paper elevation={1} sx={{ p: 3, mb: 3, borderRadius: 2 }}>
        <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', mb: 2 }}>
          <Typography variant="h4" sx={{ display: 'flex', alignItems: 'center', gap: 1, fontWeight: 'bold' }}>
            <Folder color="primary" />
            {bucket}
          </Typography>
          
          <Box sx={{ display: 'flex', gap: 1 }}>
            <Tooltip title="Refresh">
              <IconButton onClick={() => refetch()} disabled={filesLoading}>
                <Refresh />
              </IconButton>
            </Tooltip>
            <Tooltip title="Sort Options">
              <IconButton onClick={() => setSortOrder(sortOrder === 'asc' ? 'desc' : 'asc')}>
                {sortOrder === 'asc' ? <KeyboardArrowUp /> : <KeyboardArrowDown />}
              </IconButton>
            </Tooltip>
          </Box>
        </Box>
        
        {/* Breadcrumb Navigation */}
        <Breadcrumbs separator={<NavigateNext fontSize="small" />} sx={{ mb: 2 }}>
          {breadcrumbItems.map((item, index) => (
            <Link
              key={item.path}
              color={index === breadcrumbItems.length - 1 ? 'text.primary' : 'inherit'}
              underline="hover"
              sx={{ cursor: 'pointer', display: 'flex', alignItems: 'center', gap: 0.5 }}
              onClick={() => navigateToFolder(item.path)}
            >
              {index === 0 && <Home fontSize="small" />}
              {item.name}
            </Link>
          ))}
        </Breadcrumbs>
        
        {/* Bucket Stats */}
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

      {/* Search and Filters */}
      <Paper elevation={1} sx={{ p: 2, mb: 3, borderRadius: 2 }}>
        <Grid container spacing={2} alignItems="center">
          <Grid item xs={12} md={6}>
            <TextField
              fullWidth
              variant="outlined"
              placeholder="Search files and folders..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              InputProps={{
                startAdornment: <Search sx={{ mr: 1, color: 'text.secondary' }} />,
              }}
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
                  const options = ['name', 'size', 'date'];
                  const currentIndex = options.indexOf(sortBy);
                  const nextIndex = (currentIndex + 1) % options.length;
                  setSortBy(options[nextIndex] as 'name' | 'size' | 'date');
                }}
              >
                Sort: {sortBy.charAt(0).toUpperCase() + sortBy.slice(1)}
              </Button>
            </Box>
          </Grid>
        </Grid>
      </Paper>

      {/* Upload Area */}
      <Paper 
        elevation={isDragActive ? 8 : 1} 
        sx={{ 
          mb: 3, 
          borderRadius: 2,
          border: isDragActive ? '3px dashed #1976d2' : '2px dashed #e0e0e0',
          transition: 'all 0.3s ease',
          transform: isDragActive ? 'scale(1.02)' : 'scale(1)'
        }}
      >
        <CardContent>
          <Box
            {...getRootProps()}
            sx={{
              textAlign: 'center',
              py: 6,
              cursor: 'pointer',
              backgroundColor: isDragActive ? 'primary.50' : 'transparent',
              borderRadius: 1,
              transition: 'background-color 0.3s ease',
            }}
          >
            <input {...getInputProps()} />
            <CloudUpload sx={{ fontSize: 64, color: 'primary.main', mb: 2 }} />
            <Typography variant="h5" gutterBottom color="primary">
              {isDragActive ? 'Drop files here' : 'Drag & drop files here, or click to select'}
            </Typography>
            <Typography color="textSecondary" variant="body1">
              Upload files to {currentPath ? `/${currentPath}` : ''} in the {bucket} bucket
            </Typography>
          </Box>
        </CardContent>
      </Paper>

      {/* Upload Progress */}
      {uploadMutation.isPending && (
        <Alert severity="info" sx={{ mb: 2, borderRadius: 2 }}>
          <LinearProgress sx={{ mt: 1 }} />
          Uploading file...
        </Alert>
      )}

      {/* Error Messages */}
      {filesError && (
        <Alert severity="error" sx={{ mb: 2, borderRadius: 2 }}>
          Failed to load files: {filesError.message}
        </Alert>
      )}

      {uploadMutation.error && (
        <Alert severity="error" sx={{ mb: 2, borderRadius: 2 }}>
          Upload failed: {uploadMutation.error.message}
        </Alert>
      )}

      {/* Files and Folders List */}
      <Paper elevation={1} sx={{ borderRadius: 2 }}>
        <CardContent>
          <Typography variant="h6" sx={{ mb: 2, display: 'flex', alignItems: 'center', gap: 1 }}>
            <InsertDriveFile />
            {currentPath ? `Contents of /${currentPath}` : 'Root Directory'}
            <Chip 
              label={`${filteredItems.folders.length + filteredItems.files.length} items`} 
              size="small" 
              color="primary" 
              variant="outlined"
            />
          </Typography>
          
          {filesLoading ? (
            <LinearProgress />
          ) : filteredItems.folders.length === 0 && filteredItems.files.length === 0 ? (
            <Box sx={{ textAlign: 'center', py: 8 }}>
              <Folder sx={{ fontSize: 64, color: 'text.secondary', mb: 2 }} />
              <Typography color="textSecondary" variant="h6" gutterBottom>
                {searchTerm ? 'No files match your search' : 'This folder is empty'}
              </Typography>
              <Typography color="textSecondary" variant="body2">
                {searchTerm ? 'Try adjusting your search terms' : 'Upload some files to get started!'}
              </Typography>
            </Box>
          ) : (
            <List>
              {/* Folders */}
              {filteredItems.folders.map((folder, index) => (
                <React.Fragment key={folder.path}>
                  <ListItem 
                    sx={{ 
                      borderRadius: 1, 
                      mb: 0.5,
                      '&:hover': { backgroundColor: 'primary.50' }
                    }}
                  >
                    <ListItemIcon
                      onClick={() => navigateToFolder(folder.path)}
                      sx={{ cursor: 'pointer' }}
                    >
                      {getFileIcon('', true)}
                    </ListItemIcon>
                    <ListItemText
                      onClick={() => navigateToFolder(folder.path)}
                      sx={{ cursor: 'pointer' }}
                      primary={
                        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, flexWrap: 'wrap' }}>
                          <Typography variant="subtitle1" fontWeight="medium">
                            {folder.displayName}
                          </Typography>
                          <Chip 
                            label={`${folder.fileCount || 0} files`} 
                            size="small" 
                            color="primary" 
                            variant="outlined"
                          />
                          <Chip 
                            label={formatBytes(folder.size || 0)} 
                            size="small" 
                            color="secondary" 
                            variant="outlined"
                          />
                        </Box>
                      }
                      secondary={
                        <Typography variant="caption" color="textSecondary">
                          Folder • Total: {formatBytes(folder.size || 0)} • {folder.fileCount || 0} files
                        </Typography>
                      }
                    />
                    <ListItemSecondaryAction>
                      <IconButton
                        onClick={(e) => {
                          e.stopPropagation();
                          handleFolderMenuClick(e, folder);
                        }}
                        edge="end"
                      >
                        <MoreVert />
                      </IconButton>
                    </ListItemSecondaryAction>
                  </ListItem>
                  {index < filteredItems.folders.length - 1 && <Divider />}
                </React.Fragment>
              ))}

              {/* Files */}
              {filteredItems.files.map((fileItem, index) => (
                <React.Fragment key={fileItem.path}>
                  <ListItem 
                    sx={{ 
                      borderRadius: 1, 
                      mb: 0.5,
                      '&:hover': { backgroundColor: 'grey.50' }
                    }}
                  >
                    <ListItemIcon>
                      {getFileIcon(fileItem.file!.content_type)}
                    </ListItemIcon>
                    <ListItemText
                      primary={
                        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, flexWrap: 'wrap' }}>
                          <Typography variant="subtitle1" fontWeight="medium">
                            {fileItem.displayName}
                          </Typography>
                          <Chip 
                            label={formatBytes(fileItem.size!)} 
                            size="small" 
                            color="info" 
                            variant="outlined"
                          />
                          {fileItem.file!.is_compressed && (
                            <Chip label="Compressed" size="small" color="primary" />
                          )}
                          {fileItem.file!.is_encrypted && (
                            <Chip label="Encrypted" size="small" color="secondary" />
                          )}
                        </Box>
                      }
                      secondary={
                        <Box>
                          <Typography variant="body2" color="textSecondary">
                            {fileItem.file!.content_type}
                          </Typography>
                          <Typography variant="caption" color="textSecondary">
                            Uploaded: {new Date(fileItem.file!.upload_time).toLocaleDateString()} • 
                            Access Count: {fileItem.file!.access_count}
                          </Typography>
                        </Box>
                      }
                    />
                    <ListItemSecondaryAction>
                      <IconButton
                        onClick={(e) => handleMenuClick(e, fileItem.file!)}
                        edge="end"
                      >
                        <MoreVert />
                      </IconButton>
                    </ListItemSecondaryAction>
                  </ListItem>
                  {index < filteredItems.files.length - 1 && <Divider />}
                </React.Fragment>
              ))}
            </List>
          )}
        </CardContent>
      </Paper>

      {/* Floating Action Button */}
      <Fab
        color="primary"
        aria-label="upload"
        sx={{ position: 'fixed', bottom: 16, right: 16 }}
        {...getRootProps()}
      >
        <input {...getInputProps()} />
        <CloudUpload />
      </Fab>

      {/* File Menu */}
      <Menu
        anchorEl={anchorEl}
        open={Boolean(anchorEl)}
        onClose={handleMenuClose}
      >
        <MenuItem onClick={() => menuFile && handleDownload(menuFile)}>
          <ListItemIcon><Download /></ListItemIcon>
          Download
        </MenuItem>
        <MenuItem onClick={() => menuFile && handleFileInfo(menuFile)}>
          <ListItemIcon><Info /></ListItemIcon>
          File Info
        </MenuItem>
        <Divider />
        <MenuItem onClick={() => menuFile && handleDelete(menuFile)} sx={{ color: 'error.main' }}>
          <ListItemIcon><Delete color="error" /></ListItemIcon>
          Delete
        </MenuItem>
      </Menu>

      {/* Folder Menu */}
      <Menu
        anchorEl={folderMenuAnchorEl}
        open={Boolean(folderMenuAnchorEl)}
        onClose={handleFolderMenuClose}
      >
        <MenuItem onClick={() => menuFolder && navigateToFolder(menuFolder.path)}>
          <ListItemIcon><FolderOpen /></ListItemIcon>
          Open Folder
        </MenuItem>
        <Divider />
        <MenuItem onClick={() => menuFolder && handleDeleteFolder(menuFolder)} sx={{ color: 'error.main' }}>
          <ListItemIcon><DeleteForever color="error" /></ListItemIcon>
          Delete Folder & All Files
        </MenuItem>
      </Menu>

      {/* File Info Dialog */}
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
                <Typography variant="body1" gutterBottom>{formatBytes(selectedFile.file_size)}</Typography>
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
                  <Typography variant="body1" gutterBottom>{selectedFile.encryption_algorithm}</Typography>
                </Grid>
              )}
            </Grid>
          )}
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setFileInfoOpen(false)}>Close</Button>
        </DialogActions>
      </Dialog>

      {/* Delete Confirmation Dialog */}
      <Dialog open={deleteDialogOpen} onClose={() => setDeleteDialogOpen(false)}>
        <DialogTitle>Confirm Delete</DialogTitle>
        <DialogContent>
          <Typography>
            Are you sure you want to delete "{fileToDelete?.filename}"? This action cannot be undone.
          </Typography>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setDeleteDialogOpen(false)}>Cancel</Button>
          <Button onClick={confirmDelete} color="error" disabled={deleteMutation.isPending}>
            {deleteMutation.isPending ? 'Deleting...' : 'Delete'}
          </Button>
        </DialogActions>
      </Dialog>

      {/* Delete Folder Confirmation Dialog */}
      <Dialog open={deleteFolderDialogOpen} onClose={() => !isDeletingFolder && setDeleteFolderDialogOpen(false)}>
        <DialogTitle>Confirm Delete Folder</DialogTitle>
        <DialogContent>
          {!isDeletingFolder ? (
            <>
              <Alert severity="warning" sx={{ mb: 2 }}>
                This will permanently delete the folder and all its contents!
              </Alert>
              <Typography gutterBottom>
                Are you sure you want to delete the folder <strong>"{folderToDelete?.displayName}"</strong>?
              </Typography>
              <Typography variant="body2" color="textSecondary" gutterBottom>
                • This folder contains <strong>{folderToDelete?.fileCount || 0} files</strong>
              </Typography>
              <Typography variant="body2" color="textSecondary" gutterBottom>
                • Total size: <strong>{formatBytes(folderToDelete?.size || 0)}</strong>
              </Typography>
              <Typography variant="body2" color="error" sx={{ mt: 2 }}>
                This action cannot be undone!
              </Typography>
            </>
          ) : (
            <>
              <Alert severity="info" sx={{ mb: 2 }}>
                Deleting folder and all files... Please wait.
              </Alert>
              <LinearProgress sx={{ mt: 2 }} />
              <Typography variant="body2" color="textSecondary" sx={{ mt: 2, textAlign: 'center' }}>
                Deleting {folderToDelete?.fileCount || 0} files...
              </Typography>
            </>
          )}
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setDeleteFolderDialogOpen(false)} disabled={isDeletingFolder}>
            Cancel
          </Button>
          <Button 
            onClick={confirmDeleteFolder} 
            color="error" 
            variant="contained"
            disabled={isDeletingFolder}
          >
            {isDeletingFolder ? 'Deleting...' : 'Delete Folder & All Files'}
          </Button>
        </DialogActions>
      </Dialog>
    </Box>
  );
} 