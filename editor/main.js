const { app, BrowserWindow, dialog, ipcMain } = require("electron");
const fs = require("fs");
const path = require("path");

const CATEGORIES = [
  "business", "development", "education", "entertainment", "finance", "games",
  "graphics", "health", "lifestyle", "medical", "music", "news", "photo-video",
  "productivity", "reference", "social", "sports", "travel", "utility", "weather",
];

const SURFACES = ["discover", "create", "work", "play", "develop", "categories", ...CATEGORIES];

function readJson(file, fallback) {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch {
    return fallback;
  }
}

function loadCatalog(dir) {
  const feed = readJson(path.join(dir, "feed.json"), { stories: [], rails: [] });
  const images = readJson(path.join(dir, "appimages.json"), { apps: [] });
  const refs = readJson(path.join(dir, "flatpakrefs.json"), { apps: [] });
  return {
    stories: feed.stories || [],
    rails: feed.rails || [],
    appimages: images.apps || [],
    flatpakrefs: refs.apps || [],
  };
}

function validate(catalog) {
  const errors = [];
  const imageIds = new Set(catalog.appimages.map((a) => a.id));
  const refIds = new Set(catalog.flatpakrefs.map((a) => a.id));
  for (const story of catalog.stories) {
    if (!story.id || !story.title || !story.image) {
      errors.push(`Story “${story.title || story.id || "(untitled)"}” needs an id, a title, and an image.`);
    }
    if (!story.surfaces || story.surfaces.length === 0) {
      errors.push(`Story “${story.title || story.id}” needs a surface.`);
    }
    for (const surface of story.surfaces || []) {
      if (!SURFACES.includes(surface)) errors.push(`Unknown surface “${surface}”.`);
    }
  }
  const checkApp = (raw, where) => {
    if (raw.startsWith("appimage:")) {
      if (!imageIds.has(raw.slice("appimage:".length))) errors.push(`${where} names missing AppImage ${raw}.`);
    } else if (raw.startsWith("flatpakref:")) {
      if (!refIds.has(raw.slice("flatpakref:".length))) errors.push(`${where} names missing flatpakref ${raw}.`);
    } else if (!raw || raw.includes(" ")) {
      errors.push(`${where} has an empty app id.`);
    }
  };
  for (const rail of catalog.rails) {
    if (!rail.title || !rail.surface) errors.push("A rail needs a title and a surface.");
    if (rail.surface && !SURFACES.includes(rail.surface)) errors.push(`Unknown surface “${rail.surface}”.`);
    for (const id of rail.apps || []) checkApp(id, `Rail “${rail.title}”`);
  }
  for (const app of [...catalog.appimages, ...catalog.flatpakrefs]) {
    if (!app.id || !app.name || !app.download) {
      errors.push(`“${app.name || app.id || "An app"}” needs an id, a name, and a download URL.`);
    }
    for (const category of app.categories || []) {
      if (!CATEGORIES.includes(category)) errors.push(`“${app.name}” uses unknown category “${category}”.`);
    }
  }
  return errors;
}

function saveCatalog(dir, catalog) {
  const errors = validate(catalog);
  if (errors.length) return { ok: false, errors };
  fs.mkdirSync(path.join(dir, "images"), { recursive: true });
  const write = (name, value) => {
    fs.writeFileSync(path.join(dir, name), JSON.stringify(value, null, 2) + "\n");
  };
  write("feed.json", { version: 1, stories: catalog.stories, rails: catalog.rails });
  write("appimages.json", { version: 1, apps: catalog.appimages });
  write("flatpakrefs.json", { version: 1, apps: catalog.flatpakrefs });
  return { ok: true, errors: [] };
}

function createWindow() {
  const win = new BrowserWindow({
    width: 1100,
    height: 760,
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
    },
  });
  win.loadFile(path.join(__dirname, "index.html"));
}

app.whenReady().then(createWindow);

ipcMain.handle("open-dir", async () => {
  const result = await dialog.showOpenDialog({ properties: ["openDirectory"] });
  if (result.canceled || !result.filePaths[0]) return null;
  const dir = result.filePaths[0];
  return { dir, catalog: loadCatalog(dir) };
});

ipcMain.handle("save", (_event, dir, catalog) => saveCatalog(dir, catalog));

ipcMain.handle("pick-image", async (_event, dir) => {
  const result = await dialog.showOpenDialog({
    properties: ["openFile"],
    filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "svg"] }],
  });
  if (result.canceled || !result.filePaths[0]) return null;
  const source = result.filePaths[0];
  const images = path.join(dir, "images");
  fs.mkdirSync(images, { recursive: true });
  const destName = path.basename(source).replace(/[^A-Za-z0-9._-]/g, "_");
  fs.copyFileSync(source, path.join(images, destName));
  return `images/${destName}`;
});
