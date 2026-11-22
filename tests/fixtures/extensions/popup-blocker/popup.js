// Shows the address of the tab the popup acts on, as a browser's popup would see it.
chrome.tabs.query({ active: true, currentWindow: true }).then((tabs) => {
  document.getElementById('tab').textContent = tabs.length === 1 ? tabs[0].url : 'tabs: ' + tabs.length;
});
document.getElementById('close').onclick = () => window.close();
