const select = document.querySelector("#platform");
const button = document.querySelector("#download");
const versionNode = document.querySelector("#version");
const recommendation = document.querySelector("#recommendation");
const digestNode = document.querySelector("#sha256");
const errorNode = document.querySelector("#error");
const checksumsLink = document.querySelector("#checksums");
const releaseLink = document.querySelector("#release-link");
const installerTargets = [
  "linux-x64-deb", "linux-x64-rpm", "linux-arm64-deb", "linux-arm64-rpm",
  "darwin-x64-pkg", "darwin-arm64-pkg", "win32-x64-exe", "win32-arm64-exe",
];
let manifest;
let recommended;

function devicePlatform() {
  const ua = navigator.userAgentData;
  const platform = (ua?.platform || navigator.platform || "").toLowerCase();
  const arch = (ua?.architecture || "").toLowerCase();
  const arm = arch.includes("arm") || arch.includes("aarch64");
  if (platform.includes("win")) return arm ? "win32-arm64-exe" : "win32-x64-exe";
  if (platform.includes("mac")) return arm ? "darwin-arm64-pkg" : "darwin-x64-pkg";
  // A browser cannot reliably distinguish Debian-family from RPM-family Linux.
  // Do not guess package authority from architecture or installed helper tools.
  if (platform.includes("linux")) return arm ? "linux-arm64" : "linux-x64";
  return null;
}

function updateSelection() {
  const item = manifest?.installers?.[select.value];
  if (!item) {
    button.removeAttribute("href");
    button.textContent = "Download installer";
    button.classList.add("disabled");
    button.setAttribute("aria-disabled", "true");
    digestNode.textContent = "Choose an installer to view its SHA-256";
    return;
  }
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
    if (typeof manifest.version !== "string" || !manifest.installers || installerTargets.some((key) => !manifest.installers[key])) {
      throw new Error("This release does not contain the complete installer set.");
    }
    versionNode.textContent = "Version " + manifest.version;
    releaseLink.href = "https://github.com/yyjeqhc/webcodex/releases/tag/v" + encodeURIComponent(manifest.version);
    checksumsLink.href = "https://github.com/yyjeqhc/webcodex/releases/download/v" + encodeURIComponent(manifest.version) + "/SHA256SUMS";
    const options = installerTargets.map((key) => {
      const option = document.createElement("option");
      option.value = key;
      option.textContent = ({
        "linux-x64-deb": "Linux · x86-64 · Debian/Ubuntu (.deb)",
        "linux-x64-rpm": "Linux · x86-64 · RPM family (.rpm)",
        "linux-arm64-deb": "Linux · ARM64 · Debian/Ubuntu (.deb)",
        "linux-arm64-rpm": "Linux · ARM64 · RPM family (.rpm)",
        "darwin-x64-pkg": "macOS · Intel (.pkg)",
        "darwin-arm64-pkg": "macOS · Apple silicon (.pkg)",
        "win32-x64-exe": "Windows · x86-64 (.exe)",
        "win32-arm64-exe": "Windows · ARM64 (.exe)",
      })[key];
      return option;
    });
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = "Choose an installer…";
    select.replaceChildren(placeholder, ...options);
    recommended = devicePlatform();
    if (recommended?.startsWith("linux-") && !recommended.endsWith("-deb") && !recommended.endsWith("-rpm")) {
      select.value = "";
      recommendation.textContent = "Linux package family cannot be detected safely in a browser. Choose .deb for Debian/Ubuntu or .rpm for a supported RPM-family distribution.";
    } else if (recommended && manifest.installers[recommended]) {
      select.value = recommended;
      recommendation.textContent = "Recommended for this browser: " + select.options[select.selectedIndex].textContent + ". You can choose another platform.";
    } else {
      select.value = "";
      recommendation.textContent = "Choose the package matching your operating system, package family, and processor.";
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
