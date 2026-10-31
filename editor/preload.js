const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("bay", {
  openDir: () => ipcRenderer.invoke("open-dir"),
  save: (dir, catalog) => ipcRenderer.invoke("save", dir, catalog),
  pickImage: (dir) => ipcRenderer.invoke("pick-image", dir),
  categories: [
    "business", "development", "education", "entertainment", "finance", "games",
    "graphics", "health", "lifestyle", "medical", "music", "news", "photo-video",
    "productivity", "reference", "social", "sports", "travel", "utility", "weather",
  ],
  surfaces: [
    "discover", "create", "work", "play", "develop", "categories",
    "business", "development", "education", "entertainment", "finance", "games",
    "graphics", "health", "lifestyle", "medical", "music", "news", "photo-video",
    "productivity", "reference", "social", "sports", "travel", "utility", "weather",
  ],
});
