async function showStatus(action) {
  const status = document.getElementById('status');
  try {
    const response = await chrome.runtime.sendMessage({action});
    status.textContent = response?.ok ? response.message : 'Operation failed. Check the local Runner and Native Messaging installation.';
  } catch {
    status.textContent = 'The bridge is unavailable. Check the Runner, then Share this tab again.';
  }
}
for (const action of ['share', 'revoke']) {
  document.getElementById(action).addEventListener('click', () => showStatus(action));
}
void showStatus('status');
