import { runPlugin } from '@yyjeqhc/webcodex-plugin-sdk';
import { NativeBackend } from './backend.js';
import { BrowserController } from './browser.js';
import { loadConfig } from './config.js';
import { createBrowserPlugin } from './tools.js';

try {
  const backend = new NativeBackend(loadConfig());
  runPlugin(createBrowserPlugin(new BrowserController(backend)));
  let closing = false;
  const close = async (exit: boolean) => {
    if (closing) return;
    closing = true;
    await backend.shutdown();
    if (exit) process.exit(0);
  };
  process.once('beforeExit', () => { void close(false); });
  process.once('SIGTERM', () => { void close(true); });
  process.once('SIGINT', () => { void close(true); });
} catch {
  // Startup/config details may contain machine-private paths. Never echo them.
  process.stderr.write('Browser plugin startup failed. Verify the local config, SDK installation, and build.\n');
  process.exitCode = 1;
}
