import { loadSettings, saveSettings } from "./server.js";

const url = document.getElementById("url");
const token = document.getElementById("token");
const status = document.getElementById("status");

function show(text, isError = false) {
  status.textContent = text;
  status.classList.toggle("error", isError);
}

function report(message) {
  if (message?.ok) {
    const { page, anchors } = message.result;
    show(`sent to ${page}${anchors ? ` · ${anchors} anchor(s)` : ""}`);
  } else {
    show(message?.error ?? "failed", true);
  }
}

async function persist() {
  await saveSettings({ url: url.value, token: token.value });
}

async function send(cmd) {
  await persist();
  show(cmd === "capture-element" ? "pick an element on the page…" : "capturing…");
  try {
    const message = await chrome.runtime.sendMessage({ cmd });
    report(message);
  } catch (error) {
    show(String(error?.message ?? error), true);
  }
}

document.getElementById("tab").addEventListener("click", () => send("capture-tab"));
document.getElementById("element").addEventListener("click", () => send("capture-element"));
for (const input of [url, token]) input.addEventListener("change", persist);

loadSettings().then((settings) => {
  url.value = settings.url;
  token.value = settings.token;
});
