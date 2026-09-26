const select = document.querySelector("#platform");
const button = document.querySelector("#download");
const versionNode = document.querySelector("#version");
const recommendation = document.querySelector("#recommendation");
const digestNode = document.querySelector("#sha256");
const errorNode = document.querySelector("#error");
const checksumsLink = document.querySelector("#checksums");
const releaseLink = document.querySelector("#release-link");
const platforms = ["linux-x64", "linux-arm64", "darwin-x64", "darwin-arm64", "win32-x64", "win32-arm64"];
let manifest;
let recommended;

function devicePlatform() {
  const ua = navigator.userAgentData;
  const platform = (ua?.platform || navigator.platform || "").toLowerCase();
  const arch = (ua?.architecture || "").toLowerCase();
  const arm = arch.includes("arm") || arch.includes("aarch64");
  if (platform.includes("win")) return arm ? "win32-arm64" : "win32-x64";
  if (platform.includes("mac")) return arm ? "darwin-arm64" : "darwin-x64";
  if (platform.includes("linux")) return arm ? "linux-arm64" : "linux-x64";
  return null;
}

function updateSelection() {
  const item = manifest?.installers?.[select.value];
  if (!item) return;
  button.href = item.url;
  button.textContent = "Download " + select.value;
  button.classList.remove("disabled");
  button.removeAttribute("aria-disabled");
  digestNode.textContent = item.sha256;
}

async function loadManifest() {
  try {
    const response = await fetch("./manifest.json", { cache: "no-cache" });
    if (!response.ok) throw new Error("Release manifest could not be loaded.");
    manifest = await response.json();
    if (typeof manifest.version !== "string" || !manifest.installers || platforms.some((key) => !manifest.installers[key])) {
      throw new Error("This release does not contain the complete installer set.");
    }
    versionNode.textContent = "Version " + manifest.version;
    releaseLink.href = "https://github.com/yyjeqhc/webcodex/releases/tag/v" + encodeURIComponent(manifest.version);
    checksumsLink.href = "https://github.com/yyjeqhc/webcodex/releases/download/v" + encodeURIComponent(manifest.version) + "/SHA256SUMS";
    const options = platforms.map((key) => {
      const option = document.createElement("option");
      option.value = key;
      option.textContent = ({ "linux-x64": "Linux · x86-64 (.deb)", "linux-arm64": "Linux · ARM64 (.deb)",
        "darwin-x64": "macOS · Intel (.pkg)", "darwin-arm64": "macOS · Apple silicon (.pkg)",
        "win32-x64": "Windows · x86-64 (.exe)", "win32-arm64": "Windows · ARM64 (.exe)" })[key];
      return option;
    });
    select.replaceChildren(...options);
    recommended = devicePlatform();
    if (recommended && manifest.installers[recommended]) {
      select.value = recommended;
      recommendation.textContent = "Recommended for this browser: " + select.options[select.selectedIndex].textContent + ". You can choose another platform.";
    } else {
      recommendation.textContent = "Choose the option matching your operating system and processor.";
    }
    select.disabled = false;
    updateSelection();
  } catch (error) {
    errorNode.hidden = false;
    errorNode.textContent = error instanceof Error ? error.message : "Download information is unavailable.";
    versionNode.textContent = "Release information unavailable";
  }
}

select.addEventListener("change", updateSelection);
void loadManifest();
