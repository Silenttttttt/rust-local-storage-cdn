import React, { useState } from 'react';
import {
  Box, TextField, Button, Card, CardContent, Typography, List, ListItem, ListItemText,
  ListItemSecondaryAction, IconButton, Avatar, Chip, InputAdornment, Grid, Alert,
  FormControl, InputLabel, Select, MenuItem, Divider
} from '@mui/material';
import { Search as SearchIcon, Download, Clear, FilterList } from '@mui/icons-material';
import { useQuery } from '@tanstack/react-query';
import { searchFiles, downloadFile, listBuckets } from '../api/client';
import { formatBytes } from '../utils/format';
import { StoredFile } from '../types/api';

export default function Search() {
  const [query, setQuery] = useState('');
  const [selectedBucket, setSelectedBucket] = useState<string>('');
  const [searchResults, setSearchResults] = useState<StoredFile[]>([]);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Get list of buckets for filtering
  const { data: buckets = [] } = useQuery({
    queryKey: ['buckets'],
    queryFn: listBuckets,
  });

  const handleSearch = async () => {
    if (!query.trim()) return;

    setSearching(true);
    setError(null);
    
    try {
      const results = await searchFiles({
        query: query.trim(),
        bucket: selectedBucket || undefined,
        limit: 100,
      });
      setSearchResults(results);
    } catch (err: any) {
      setError(err.message || 'Search failed');
      setSearchResults([]);
    } finally {
      setSearching(false);
    }
  };

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
    } catch (error) {
      console.error('Download failed:', error);
    }
  };

  const handleClear = () => {
    setQuery('');
    setSelectedBucket('');
    setSearchResults([]);
    setError(null);
  };

  const getFileIcon = (contentType: string) => {
    if (contentType.startsWith('image/')) return '🖼️';
    if (contentType.startsWith('video/')) return '🎥';
    if (contentType.startsWith('audio/')) return '🎵';
    if (contentType.includes('pdf')) return '📄';
    if (contentType.includes('text')) return '📝';
    if (contentType.includes('zip') || contentType.includes('archive')) return '📦';
    return '📁';
  };

  const highlightText = (text: string, highlight: string) => {
    if (!highlight.trim()) return text;
    
    const regex = new RegExp(`(${highlight})`, 'gi');
    const parts = text.split(regex);
    
    return parts.map((part, index) =>
      regex.test(part) ? (
        <Box component="span" key={index} sx={{ backgroundColor: 'yellow', fontWeight: 'bold' }}>
          {part}
        </Box>
      ) : (
        part
      )
    );
  };

  return (
    <Box sx={{ p: 3 }}>
      <Typography variant="h4" gutterBottom sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
        <SearchIcon color="primary" />
        File Search
      </Typography>

      {/* Search Form */}
      <Card sx={{ mb: 3 }}>
        <CardContent>
          <Grid container spacing={2} alignItems="center">
            <Grid item xs={12} md={6}>
              <TextField
                fullWidth
                variant="outlined"
                placeholder="Search files by name, content type, or metadata..."
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyPress={(e) => e.key === 'Enter' && handleSearch()}
                InputProps={{
                  startAdornment: (
                    <InputAdornment position="start">
                      <SearchIcon />
                    </InputAdornment>
                  ),
                }}
              />
            </Grid>
            
            <Grid item xs={12} md={3}>
              <FormControl fullWidth>
                <InputLabel>Filter by Bucket</InputLabel>
                <Select
                  value={selectedBucket}
                  onChange={(e) => setSelectedBucket(e.target.value)}
                  label="Filter by Bucket"
                  startAdornment={<FilterList sx={{ mr: 1 }} />}
                >
                  <MenuItem value="">All Buckets</MenuItem>
                  {buckets.map((bucket) => (
                    <MenuItem key={bucket} value={bucket}>
                      {bucket}
                    </MenuItem>
                  ))}
                </Select>
              </FormControl>
            </Grid>
            
            <Grid item xs={12} md={3}>
              <Box sx={{ display: 'flex', gap: 1 }}>
                <Button
                  variant="contained"
                  onClick={handleSearch}
                  disabled={!query.trim() || searching}
                  fullWidth
                >
                  {searching ? 'Searching...' : 'Search'}
                </Button>
                <IconButton onClick={handleClear}>
                  <Clear />
                </IconButton>
              </Box>
            </Grid>
          </Grid>
          
          <Typography variant="body2" color="textSecondary" sx={{ mt: 1 }}>
            Search across filenames, content types, and metadata. Use quotes for exact phrases.
          </Typography>
        </CardContent>
      </Card>

      {/* Error Message */}
      {error && (
        <Alert severity="error" sx={{ mb: 2 }}>
          {error}
        </Alert>
      )}

      {/* Search Results */}
      {searchResults.length > 0 && (
        <Card>
          <CardContent>
            <Typography variant="h6" gutterBottom>
              Search Results ({searchResults.length} files found)
            </Typography>
            
            <List>
              {searchResults.map((file, index) => (
                <React.Fragment key={file.id}>
                  <ListItem>
                    <Avatar sx={{ mr: 2, bgcolor: 'transparent' }}>
                      {getFileIcon(file.content_type)}
                    </Avatar>
                    <ListItemText
                      primary={
                        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, flexWrap: 'wrap' }}>
                          <Typography variant="subtitle1">
                            {highlightText(file.filename, query)}
                          </Typography>
                          <Chip label={file.bucket} size="small" variant="outlined" />
                          {file.is_compressed && <Chip label="Compressed" size="small" color="primary" />}
                          {file.is_encrypted && <Chip label="Encrypted" size="small" color="secondary" />}
                        </Box>
                      }
                      secondary={
                        <Box>
                          <Typography variant="body2" color="textSecondary">
                            {formatBytes(file.file_size)} • {highlightText(file.content_type, query)}
                          </Typography>
                                                     <Typography variant="caption" color="textSecondary">
                             Key: {highlightText(file.key, query || '')} • 
                             Uploaded: {new Date(file.upload_time).toLocaleDateString()} • 
                             Access Count: {file.access_count}
                           </Typography>
                          {file.last_accessed && (
                            <Typography variant="caption" color="textSecondary" sx={{ display: 'block' }}>
                              Last accessed: {new Date(file.last_accessed).toLocaleDateString()}
                            </Typography>
                          )}
                        </Box>
                      }
                    />
                    <ListItemSecondaryAction>
                      <IconButton onClick={() => handleDownload(file)} edge="end">
                        <Download />
                      </IconButton>
                    </ListItemSecondaryAction>
                  </ListItem>
                  {index < searchResults.length - 1 && <Divider />}
                </React.Fragment>
              ))}
            </List>
          </CardContent>
        </Card>
      )}

      {/* No Results */}
      {!searching && query && searchResults.length === 0 && !error && (
        <Card>
          <CardContent sx={{ textAlign: 'center', py: 6 }}>
            <SearchIcon sx={{ fontSize: 64, color: 'text.secondary', mb: 2 }} />
            <Typography variant="h6" gutterBottom>
              No files found
            </Typography>
            <Typography color="textSecondary">
              Try adjusting your search terms or removing the bucket filter.
            </Typography>
          </CardContent>
        </Card>
      )}

      {/* Empty State */}
      {!query && searchResults.length === 0 && (
        <Card>
          <CardContent sx={{ textAlign: 'center', py: 6 }}>
            <SearchIcon sx={{ fontSize: 64, color: 'text.secondary', mb: 2 }} />
            <Typography variant="h6" gutterBottom>
              Search Files
            </Typography>
            <Typography color="textSecondary">
              Enter a search term to find files across all your buckets.
            </Typography>
          </CardContent>
        </Card>
      )}
    </Box>
  );
} 