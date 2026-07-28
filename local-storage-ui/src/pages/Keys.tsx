import { useState } from 'react';
import {
  Box, Typography, Card, CardContent, List, ListItem, ListItemText, ListItemIcon,
  ListItemSecondaryAction, IconButton, Chip, Button, Dialog, DialogTitle, DialogContent,
  DialogActions, TextField, FormControl, InputLabel, Select, MenuItem, Alert, Divider,
} from '@mui/material';
import { Key, Add, Block } from '@mui/icons-material';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { listEncryptionKeys, createEncryptionKey, deactivateEncryptionKey } from '../api/client';
import { CreateKeyRequest, EncryptionKeyInfo } from '../types/api';

export default function Keys() {
  const queryClient = useQueryClient();
  const [createOpen, setCreateOpen] = useState(false);
  const [form, setForm] = useState<CreateKeyRequest>({ algorithm: 'aes-gcm', description: '' });
  const [keyToDeactivate, setKeyToDeactivate] = useState<string | null>(null);

  const { data: keys = [], isLoading, error } = useQuery<EncryptionKeyInfo[]>({
    queryKey: ['encryption-keys'],
    queryFn: listEncryptionKeys,
  });

  const createMutation = useMutation({
    mutationFn: createEncryptionKey,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['encryption-keys'] });
      setCreateOpen(false);
      setForm({ algorithm: 'aes-gcm', description: '' });
    },
  });

  const deactivateMutation = useMutation({
    mutationFn: deactivateEncryptionKey,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['encryption-keys'] });
      setKeyToDeactivate(null);
    },
  });

  return (
    <Box sx={{ p: 3 }}>
      <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', mb: 2 }}>
        <Typography variant="h4" sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
          <Key color="primary" />
          Encryption Keys
        </Typography>
        <Button variant="contained" startIcon={<Add />} onClick={() => setCreateOpen(true)}>
          New Key
        </Button>
      </Box>

      <Alert severity="info" sx={{ mb: 3 }}>
        Pass a key's ID as <code>encryption_key_id</code> when uploading (see a bucket's "Upload options")
        to encrypt that file with it instead of the server's global key. Raw key material is never returned
        by the API once a key is created.
      </Alert>

      {error instanceof Error && (
        <Alert severity="error" sx={{ mb: 2 }}>Failed to load keys: {error.message}</Alert>
      )}
      {createMutation.error instanceof Error && (
        <Alert severity="error" sx={{ mb: 2 }}>Failed to create key: {createMutation.error.message}</Alert>
      )}

      <Card>
        <CardContent>
          {isLoading ? (
            <Typography>Loading keys...</Typography>
          ) : keys.length === 0 ? (
            <Box sx={{ textAlign: 'center', py: 6 }}>
              <Key sx={{ fontSize: 64, color: 'text.secondary', mb: 2 }} />
              <Typography variant="h6" gutterBottom>No encryption keys yet</Typography>
              <Typography color="textSecondary">
                Files uploaded with encryption use the server's global key until you create one.
              </Typography>
            </Box>
          ) : (
            <List>
              {keys.map((key: EncryptionKeyInfo, index: number) => (
                <Box key={key.key_id}>
                  <ListItem>
                    <ListItemIcon><Key color={key.is_active ? 'primary' : 'disabled'} /></ListItemIcon>
                    <ListItemText
                      primaryTypographyProps={{ component: 'div' }}
                      secondaryTypographyProps={{ component: 'div' }}
                      primary={
                        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                          <Typography variant="subtitle1" sx={{ fontFamily: 'monospace' }}>
                            {key.key_id}
                          </Typography>
                          <Chip label={key.algorithm} size="small" color="primary" variant="outlined" />
                          {!key.is_active && <Chip label="Inactive" size="small" color="default" />}
                        </Box>
                      }
                      secondary={
                        <>
                          {key.description && <Typography variant="body2" color="textSecondary">{key.description}</Typography>}
                          {key.created_at && (
                            <Typography variant="caption" color="textSecondary">
                              Created {new Date(key.created_at).toLocaleString()}
                            </Typography>
                          )}
                        </>
                      }
                    />
                    {key.is_active && (
                      <ListItemSecondaryAction>
                        <IconButton onClick={() => setKeyToDeactivate(key.key_id)} edge="end" title="Deactivate">
                          <Block />
                        </IconButton>
                      </ListItemSecondaryAction>
                    )}
                  </ListItem>
                  {index < keys.length - 1 && <Divider />}
                </Box>
              ))}
            </List>
          )}
        </CardContent>
      </Card>

      <Dialog open={createOpen} onClose={() => setCreateOpen(false)} fullWidth maxWidth="sm">
        <DialogTitle>New Encryption Key</DialogTitle>
        <DialogContent>
          <FormControl fullWidth sx={{ mt: 1, mb: 2 }}>
            <InputLabel>Algorithm</InputLabel>
            <Select
              label="Algorithm"
              value={form.algorithm}
              onChange={(e) => setForm((f) => ({ ...f, algorithm: e.target.value as CreateKeyRequest['algorithm'] }))}
            >
              <MenuItem value="aes-gcm">AES-256-GCM</MenuItem>
              <MenuItem value="chacha20poly1305">ChaCha20-Poly1305</MenuItem>
            </Select>
          </FormControl>
          <TextField
            fullWidth
            label="Description (optional)"
            value={form.description}
            onChange={(e) => setForm((f) => ({ ...f, description: e.target.value }))}
          />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setCreateOpen(false)}>Cancel</Button>
          <Button variant="contained" disabled={createMutation.isPending} onClick={() => createMutation.mutate(form)}>
            {createMutation.isPending ? 'Creating...' : 'Create'}
          </Button>
        </DialogActions>
      </Dialog>

      <Dialog open={!!keyToDeactivate} onClose={() => setKeyToDeactivate(null)}>
        <DialogTitle>Deactivate Key</DialogTitle>
        <DialogContent>
          <Alert severity="warning" sx={{ mb: 2 }}>
            Files already encrypted with this key remain readable - deactivating only prevents new uploads from using it.
          </Alert>
          <Typography>Deactivate key "{keyToDeactivate}"?</Typography>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setKeyToDeactivate(null)}>Cancel</Button>
          <Button
            color="error"
            disabled={deactivateMutation.isPending}
            onClick={() => keyToDeactivate && deactivateMutation.mutate(keyToDeactivate)}
          >
            {deactivateMutation.isPending ? 'Deactivating...' : 'Deactivate'}
          </Button>
        </DialogActions>
      </Dialog>
    </Box>
  );
}
