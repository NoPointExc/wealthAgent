import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig(({ command, mode }) => {
  // Guard: a production build MUST have the Google OAuth client ID, or the
  // bundle ships with clientId="" and sign-in fails with
  // "Error 400: invalid_request". Fail loudly instead of shipping a broken
  // bundle. (dev/serve is exempt — login isn't always exercised locally.)
  if (command === 'build') {
    const env = loadEnv(mode, process.cwd())
    if (!env.VITE_GOOGLE_CLIENT_ID) {
      throw new Error(
        '\n\nBUILD ABORTED: VITE_GOOGLE_CLIENT_ID is not set.\n' +
          'The production bundle needs the Google OAuth client ID baked in, or\n' +
          'sign-in breaks with "Error 400: invalid_request".\n' +
          'Build via ops/build-push.sh (which injects it), or pass it explicitly:\n' +
          '  VITE_GOOGLE_CLIENT_ID=<your-client-id> npm run build\n',
      )
    }
  }

  return {
    plugins: [react()],
    server: {
      proxy: {
        '/api': 'http://localhost:8080',
      },
    },
  }
})
