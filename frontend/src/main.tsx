import React from 'react'
import ReactDOM from 'react-dom/client'
import { GoogleOAuthProvider } from '@react-oauth/google'
import App from './App.tsx'
import { PrivacyProvider } from './context/PrivacyContext'
import { ConfigProvider } from './context/ConfigContext'
import './index.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <GoogleOAuthProvider clientId={import.meta.env.VITE_GOOGLE_CLIENT_ID}>
      <ConfigProvider>
        <PrivacyProvider>
          <App />
        </PrivacyProvider>
      </ConfigProvider>
    </GoogleOAuthProvider>
  </React.StrictMode>,
)
