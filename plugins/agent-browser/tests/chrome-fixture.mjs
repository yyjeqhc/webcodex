import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { once } from 'node:events';

export const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
export const chromeExecutable = process.env.WC_TEST_CHROME || (process.platform === 'darwin'
  ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
  : process.platform === 'win32' ? 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe' : 'google-chrome');

export async function startFixture({ headed = false } = {}) {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'wc-browser-test-'));
  const server = http.createServer((request, response) => {
    response.setHeader('Content-Type', 'text/html; charset=utf-8');
    response.setHeader('Cache-Control', 'no-store');
    if (request.url?.startsWith('/large')) {
      const paragraphs = Array.from({ length: 90 }, (_, i) => `<p>Static paragraph ${i}: ${'context '.repeat(12)}</p>`).join('');
      const links = Array.from({ length: 12 }, (_, i) => `<a href="#item-${i}">Action ${i}</a>`).join(' ');
      response.end(`<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Large browser test</title></head><body><h1>Large page</h1>${paragraphs}<nav>${links}</nav></body></html>`);
      return;
    }
    response.end(`<!doctype html><html lang="en"><head><meta charset="utf-8"><title>WebCodex browser test</title>
      <style>body{font:20px system-ui;max-width:800px;margin:60px auto}input,button,select{font:inherit;padding:10px;margin:8px}#result{padding:16px;border:1px solid} .space{height:1200px}</style></head>
      <body><h1>WebCodex agent-browser live test</h1><p>Isolated test profile. No personal accounts.</p>
      <label>Name <input id="name" autocomplete="off"></label><button id="greet">Greet</button>
      <label>Plan <select id="plan"><option value="starter">Starter</option><option value="pro">Pro</option></select></label>
      <label><input id="agree" type="checkbox"> Agree</label>
      <p id="result" role="status">Ready</p><button id="replace">Replace action</button>
      <a href="/second">Second page</a><button id="reload">Reload same URL</button>
      <div class="space"></div><p>End of fixture</p><script>
      document.querySelector('#greet').onclick=()=>document.querySelector('#result').textContent='Hello '+document.querySelector('#name').value;
      document.querySelector('#replace').onclick=()=>document.querySelector('#greet').textContent='Changed action';
      document.querySelector('#reload').onclick=()=>location.reload();
      </script></body></html>`);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const args = ['--remote-debugging-port=0', '--remote-debugging-address=127.0.0.1',
    '--user-data-dir=' + path.join(dir, 'chrome'), '--no-first-run', '--no-default-browser-check',
    '--disable-background-networking', ...(headed ? [] : ['--headless=new']), 'about:blank'];
  const chrome = spawn(chromeExecutable, args, { stdio: ['ignore', 'ignore', 'ignore'] });
  let launchError;
  chrome.on('error', error => { launchError = error; });
  const cleanup = async () => {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    if (chrome.exitCode === null && chrome.signalCode === null && !launchError) {
      const exited = once(chrome, 'exit').catch(() => {});
      chrome.kill('SIGTERM');
      await Promise.race([exited, sleep(3000)]);
      if (chrome.exitCode === null && chrome.signalCode === null) {
        chrome.kill('SIGKILL');
        await Promise.race([exited, sleep(2000)]);
      }
    }
    await fs.rm(dir, { recursive: true, force: true, maxRetries: 8, retryDelay: 250 });
  };
  try {
    for (let i = 0; i < 150; i++) {
      if (launchError) throw launchError;
      if (chrome.exitCode !== null) throw new Error('Isolated test Chrome exited before readiness.');
      try {
        const port = (await fs.readFile(path.join(dir, 'chrome', 'DevToolsActivePort'), 'utf8')).split('\n')[0];
        if (!/^\d+$/.test(port)) throw new Error('Invalid test debugging port.');
        if ((await fetch(`http://127.0.0.1:${port}/json/version`)).ok) {
          return { dir, port, url: `http://127.0.0.1:${server.address().port}`, chrome, cleanup };
        }
      } catch {}
      await sleep(100);
    }
    throw new Error('Isolated test Chrome did not become ready.');
  } catch (error) { await cleanup(); throw error; }
}
