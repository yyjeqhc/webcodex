for (const action of ['share', 'revoke']) {
  document.getElementById(action).addEventListener('click', async () => {
    const status = document.getElementById('status');
    try {
      const response = await chrome.runtime.sendMessage({action});
      status.textContent = response?.ok ? response.message : 'Operation failed. Check the local Runner and Native Messaging installation.';
    } catch {
      status.textContent = 'The bridge is unavailable. No tab was attached.';
    }
  });
}
