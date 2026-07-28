import { useState } from 'react';
import { Link, useLocation } from 'react-router-dom';
import {
  AppBar, Toolbar, Typography, Box, Drawer, List, ListItem, ListItemButton,
  ListItemText, ListItemIcon, IconButton, useTheme, useMediaQuery, Divider,
  Badge, Chip, Switch, FormControlLabel,
} from '@mui/material';
import {
  Menu as MenuIcon, Dashboard, Storage, Search, Folder,
  Key, DarkMode, LightMode,
} from '@mui/icons-material';
import { useQuery } from '@tanstack/react-query';
import { getHealth, getStorageStats } from '../api/client';

const drawerWidth = 280;

interface LayoutProps {
  children: React.ReactNode;
  mode: 'light' | 'dark';
  setMode: (mode: 'light' | 'dark') => void;
}

const pageTitles: Record<string, string> = {
  '/': 'Dashboard',
  '/dashboard': 'Dashboard',
  '/buckets': 'Buckets',
  '/search': 'Search',
  '/keys': 'Encryption Keys',
};

export default function Layout({ children, mode, setMode }: LayoutProps) {
  const [mobileOpen, setMobileOpen] = useState(false);
  const location = useLocation();
  const theme = useTheme();
  const isMobile = useMediaQuery(theme.breakpoints.down('md'));

  const { data: isHealthy } = useQuery({
    queryKey: ['health'],
    queryFn: getHealth,
    refetchInterval: 30000,
  });

  const { data: stats } = useQuery({
    queryKey: ['storage-stats'],
    queryFn: getStorageStats,
    refetchInterval: 60000,
  });

  const handleDrawerToggle = () => setMobileOpen((open) => !open);
  const handleModeToggle = () => setMode(mode === 'light' ? 'dark' : 'light');

  const menuItems = [
    { text: 'Dashboard', icon: <Dashboard />, path: '/dashboard', description: 'Overview and statistics' },
    {
      text: 'Buckets', icon: <Storage />, path: '/buckets', description: 'Manage storage buckets',
      badge: stats?.total_files,
    },
    { text: 'Search', icon: <Search />, path: '/search', description: 'Search files across buckets' },
    { text: 'Encryption Keys', icon: <Key />, path: '/keys', description: 'Manage per-file encryption keys' },
  ];

  const isActiveRoute = (path: string) => {
    if (path === '/dashboard' && location.pathname === '/') return true;
    return location.pathname === path || location.pathname.startsWith(path + '/');
  };

  const pageTitle = pageTitles[location.pathname] ?? 'Local Storage';

  const drawer = (
    <Box sx={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <Toolbar>
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
          <Folder color="primary" />
          <Typography variant="h6" noWrap component="div">
            Local Storage
          </Typography>
        </Box>
      </Toolbar>
      <Divider />

      <Box sx={{ p: 2 }}>
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 1 }}>
          <Box
            sx={{
              width: 8,
              height: 8,
              borderRadius: '50%',
              backgroundColor: isHealthy ? 'success.main' : 'error.main',
            }}
          />
          <Typography variant="body2" color="textSecondary">
            Backend {isHealthy ? 'Online' : 'Offline'}
          </Typography>
        </Box>

        {stats && (
          <Box sx={{ display: 'flex', gap: 1, flexWrap: 'wrap' }}>
            <Chip label={`${stats.total_files} files`} size="small" variant="outlined" />
            {stats.compressed_files > 0 && (
              <Chip label={`${stats.compressed_files} compressed`} size="small" color="primary" variant="outlined" />
            )}
          </Box>
        )}
      </Box>

      <Divider />

      <List>
        {menuItems.map((item) => (
          <ListItem key={item.text} disablePadding>
            <ListItemButton
              component={Link}
              to={item.path}
              selected={isActiveRoute(item.path)}
              onClick={() => isMobile && setMobileOpen(false)}
              sx={{
                borderRadius: 1,
                mx: 1,
                mb: 0.5,
                '&.Mui-selected': {
                  backgroundColor: 'primary.main',
                  color: 'primary.contrastText',
                  '&:hover': { backgroundColor: 'primary.dark' },
                  '& .MuiListItemIcon-root': { color: 'primary.contrastText' },
                },
              }}
            >
              <ListItemIcon sx={{ minWidth: 40 }}>
                {item.badge ? (
                  <Badge badgeContent={item.badge > 999 ? '999+' : item.badge} color="secondary">
                    {item.icon}
                  </Badge>
                ) : (
                  item.icon
                )}
              </ListItemIcon>
              <ListItemText
                primary={item.text}
                secondary={item.description}
                secondaryTypographyProps={{ variant: 'caption', sx: { opacity: 0.7 } }}
              />
            </ListItemButton>
          </ListItem>
        ))}
      </List>

      <Divider sx={{ mt: 'auto' }} />

      <Box sx={{ p: 2 }}>
        <FormControlLabel
          control={
            <Switch
              checked={mode === 'dark'}
              onChange={handleModeToggle}
              icon={<LightMode />}
              checkedIcon={<DarkMode />}
            />
          }
          label={
            <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
              {mode === 'dark' ? <DarkMode fontSize="small" /> : <LightMode fontSize="small" />}
              <Typography variant="body2">{mode === 'dark' ? 'Dark' : 'Light'} Mode</Typography>
            </Box>
          }
        />
      </Box>
    </Box>
  );

  return (
    <Box sx={{ display: 'flex' }}>
      <AppBar
        position="fixed"
        sx={{ width: { md: `calc(100% - ${drawerWidth}px)` }, ml: { md: `${drawerWidth}px` } }}
      >
        <Toolbar>
          <IconButton
            color="inherit"
            aria-label="open drawer"
            edge="start"
            onClick={handleDrawerToggle}
            sx={{ mr: 2, display: { md: 'none' } }}
          >
            <MenuIcon />
          </IconButton>
          <Typography variant="h6" noWrap component="div" sx={{ flexGrow: 1 }}>
            {pageTitle}
          </Typography>

          <Box sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
            {stats && (
              <Typography variant="body2" sx={{ display: { xs: 'none', sm: 'block' } }}>
                {stats.total_files.toLocaleString()} files
              </Typography>
            )}
            <Box
              sx={{
                width: 8,
                height: 8,
                borderRadius: '50%',
                backgroundColor: isHealthy ? 'success.light' : 'error.light',
              }}
            />
            <IconButton color="inherit" onClick={handleModeToggle}>
              {mode === 'dark' ? <LightMode /> : <DarkMode />}
            </IconButton>
          </Box>
        </Toolbar>
      </AppBar>

      <Box component="nav" sx={{ width: { md: drawerWidth }, flexShrink: { md: 0 } }}>
        <Drawer
          variant="temporary"
          open={mobileOpen}
          onClose={handleDrawerToggle}
          ModalProps={{ keepMounted: true }}
          sx={{
            display: { xs: 'block', md: 'none' },
            '& .MuiDrawer-paper': { boxSizing: 'border-box', width: drawerWidth },
          }}
        >
          {drawer}
        </Drawer>

        <Drawer
          variant="permanent"
          sx={{
            display: { xs: 'none', md: 'block' },
            '& .MuiDrawer-paper': { boxSizing: 'border-box', width: drawerWidth },
          }}
          open
        >
          {drawer}
        </Drawer>
      </Box>

      <Box
        component="main"
        sx={{
          flexGrow: 1,
          width: { md: `calc(100% - ${drawerWidth}px)` },
          minHeight: '100vh',
          backgroundColor: 'background.default',
        }}
      >
        <Toolbar />
        {children}
      </Box>
    </Box>
  );
}
