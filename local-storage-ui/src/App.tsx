import { BrowserRouter as Router, Routes, Route } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ThemeProvider, createTheme } from '@mui/material/styles';
import { CssBaseline } from '@mui/material';
import { useState, useMemo } from 'react';
import Layout from './components/Layout';
import Dashboard from './pages/Dashboard';
import BucketView from './pages/BucketView';
import Search from './pages/Search';
import Buckets from './pages/Buckets';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

function App() {
  const [mode, setMode] = useState<'light' | 'dark'>('dark');

  const theme = useMemo(
    () =>
      createTheme({
        palette: {
          mode,
          primary: {
            main: mode === 'dark' ? '#64b5f6' : '#2196f3', // Softer blue
            light: mode === 'dark' ? '#90caf9' : '#64b5f6',
            dark: mode === 'dark' ? '#1976d2' : '#1976d2',
          },
          secondary: {
            main: mode === 'dark' ? '#81c784' : '#4caf50', // Softer green
            light: mode === 'dark' ? '#a5d6a7' : '#81c784',
            dark: mode === 'dark' ? '#388e3c' : '#388e3c',
          },
          success: {
            main: mode === 'dark' ? '#66bb6a' : '#4caf50', // Muted green
            light: mode === 'dark' ? '#a5d6a7' : '#81c784',
            dark: mode === 'dark' ? '#388e3c' : '#388e3c',
          },
          warning: {
            main: mode === 'dark' ? '#ffb74d' : '#ff9800', // Muted orange
            light: mode === 'dark' ? '#ffcc02' : '#ffb74d',
            dark: mode === 'dark' ? '#f57c00' : '#f57c00',
          },
          error: {
            main: mode === 'dark' ? '#e57373' : '#f44336', // Muted red
            light: mode === 'dark' ? '#ef9a9a' : '#e57373',
            dark: mode === 'dark' ? '#d32f2f' : '#d32f2f',
          },
          info: {
            main: mode === 'dark' ? '#4fc3f7' : '#00bcd4', // Muted cyan
            light: mode === 'dark' ? '#81d4fa' : '#4fc3f7',
            dark: mode === 'dark' ? '#0288d1' : '#0288d1',
          },
          background: {
            default: mode === 'dark' ? '#0a0a0a' : '#fafafa', // Much darker background
            paper: mode === 'dark' ? '#1a1a1a' : '#ffffff', // Softer paper color
          },
          text: {
            primary: mode === 'dark' ? '#e0e0e0' : '#212121', // Softer white
            secondary: mode === 'dark' ? '#9e9e9e' : '#757575', // Muted gray
          },
          divider: mode === 'dark' ? '#2a2a2a' : '#e0e0e0',
        },
        components: {
          MuiCard: {
            styleOverrides: {
              root: {
                backgroundColor: mode === 'dark' ? '#1a1a1a' : '#ffffff',
                border: mode === 'dark' ? '1px solid #2a2a2a' : '1px solid #e0e0e0',
                borderRadius: 12,
                boxShadow: mode === 'dark' 
                  ? '0 4px 20px rgba(0, 0, 0, 0.3)' 
                  : '0 4px 20px rgba(0, 0, 0, 0.1)',
              },
            },
          },
          MuiPaper: {
            styleOverrides: {
              root: {
                backgroundColor: mode === 'dark' ? '#1a1a1a' : '#ffffff',
                border: mode === 'dark' ? '1px solid #2a2a2a' : '1px solid #e0e0e0',
                borderRadius: 12,
                boxShadow: mode === 'dark' 
                  ? '0 4px 20px rgba(0, 0, 0, 0.3)' 
                  : '0 4px 20px rgba(0, 0, 0, 0.1)',
              },
            },
          },
          MuiDrawer: {
            styleOverrides: {
              paper: {
                backgroundColor: mode === 'dark' ? '#0f0f0f' : '#ffffff',
                borderRight: mode === 'dark' ? '1px solid #2a2a2a' : '1px solid #e0e0e0',
              },
            },
          },
          MuiAppBar: {
            styleOverrides: {
              root: {
                backgroundColor: mode === 'dark' ? '#0f0f0f' : '#2196f3',
                borderBottom: mode === 'dark' ? '1px solid #2a2a2a' : 'none',
                boxShadow: mode === 'dark' 
                  ? '0 2px 10px rgba(0, 0, 0, 0.3)' 
                  : '0 2px 10px rgba(0, 0, 0, 0.1)',
              },
            },
          },
          MuiButton: {
            styleOverrides: {
              root: {
                borderRadius: 8,
                textTransform: 'none',
                fontWeight: 500,
                boxShadow: mode === 'dark' 
                  ? '0 2px 8px rgba(0, 0, 0, 0.2)' 
                  : '0 2px 8px rgba(0, 0, 0, 0.1)',
                '&:hover': {
                  boxShadow: mode === 'dark' 
                    ? '0 4px 12px rgba(0, 0, 0, 0.3)' 
                    : '0 4px 12px rgba(0, 0, 0, 0.15)',
                },
              },
              contained: {
                backgroundColor: mode === 'dark' ? '#64b5f6' : '#2196f3',
                '&:hover': {
                  backgroundColor: mode === 'dark' ? '#90caf9' : '#1976d2',
                },
              },
            },
          },
          MuiChip: {
            styleOverrides: {
              root: {
                borderRadius: 16,
                fontWeight: 500,
              },
            },
          },
          MuiTextField: {
            styleOverrides: {
              root: {
                '& .MuiOutlinedInput-root': {
                  borderRadius: 8,
                  backgroundColor: mode === 'dark' ? '#2a2a2a' : '#f5f5f5',
                  '&:hover': {
                    backgroundColor: mode === 'dark' ? '#333333' : '#eeeeee',
                  },
                  '&.Mui-focused': {
                    backgroundColor: mode === 'dark' ? '#333333' : '#ffffff',
                  },
                },
              },
            },
          },
          MuiListItem: {
            styleOverrides: {
              root: {
                borderRadius: 8,
                marginBottom: 4,
                '&:hover': {
                  backgroundColor: mode === 'dark' ? '#2a2a2a' : '#f5f5f5',
                },
              },
            },
          },
          MuiFab: {
            styleOverrides: {
              root: {
                backgroundColor: mode === 'dark' ? '#64b5f6' : '#2196f3',
                color: '#ffffff',
                boxShadow: mode === 'dark' 
                  ? '0 4px 20px rgba(100, 181, 246, 0.3)' 
                  : '0 4px 20px rgba(33, 150, 243, 0.3)',
                '&:hover': {
                  backgroundColor: mode === 'dark' ? '#90caf9' : '#1976d2',
                  boxShadow: mode === 'dark' 
                    ? '0 6px 25px rgba(100, 181, 246, 0.4)' 
                    : '0 6px 25px rgba(33, 150, 243, 0.4)',
                },
              },
            },
          },
          MuiAlert: {
            styleOverrides: {
              root: {
                borderRadius: 8,
                border: 'none',
              },
            },
          },
        },
        shape: {
          borderRadius: 8,
        },
        typography: {
          fontFamily: '"Inter", "Roboto", "Helvetica", "Arial", sans-serif',
          h4: {
            fontWeight: 600,
          },
          h6: {
            fontWeight: 600,
          },
          subtitle1: {
            fontWeight: 500,
          },
        },
      }),
    [mode]
  );

  return (
    <QueryClientProvider client={queryClient}>
      <ThemeProvider theme={theme}>
        <CssBaseline />
        <Router>
          <Layout mode={mode} setMode={setMode}>
            <Routes>
              <Route path="/" element={<Dashboard />} />
              <Route path="/dashboard" element={<Dashboard />} />
              <Route path="/buckets" element={<Buckets />} />
              <Route path="/buckets/:bucket" element={<BucketView />} />
              <Route path="/search" element={<Search />} />
            </Routes>
          </Layout>
        </Router>
      </ThemeProvider>
    </QueryClientProvider>
  );
}

export default App; 