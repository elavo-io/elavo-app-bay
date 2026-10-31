const state = {
  dir: "",
  section: "stories",
  index: -1,
  catalog: { stories: [], rails: [], appimages: [], flatpakrefs: [] },
};

const list = document.querySelector("#list");
const form = document.querySelector("#form");
const status = document.querySelector("#status");
const dirLabel = document.querySelector("#dir");

function items() {
  return state.catalog[state.section];
}

function blank() {
  if (state.section === "stories") {
    return { id: "", kicker: "", title: "", subtitle: "", body: "", image: "", color: "#243044", date: "", style: "card", surfaces: ["discover"], apps: [] };
  }
  if (state.section === "rails") {
    return { id: "", title: "", surface: "discover", apps: [] };
  }
  return { id: "", name: "", summary: "", developer: "", categories: [], icon: "", download: "", version: "", homepage: "" };
}

function showStatus(text) {
  status.textContent = text;
}

function renderList() {
  list.replaceChildren();
  items().forEach((item, index) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "item";
    button.textContent = item.title || item.name || item.id || "(untitled)";
    button.addEventListener("click", () => {
      state.index = index;
      renderForm();
    });
    list.append(button);
  });
  const add = document.createElement("button");
  add.type = "button";
  add.textContent = "Add";
  add.addEventListener("click", () => {
    items().push(blank());
    state.index = items().length - 1;
    renderList();
    renderForm();
  });
  list.append(add);
}

function field(label, value, onInput, multiline) {
  const wrap = document.createElement("label");
  wrap.textContent = label;
  const input = document.createElement(multiline ? "textarea" : "input");
  input.value = value || "";
  input.addEventListener("input", () => onInput(input.value));
  wrap.append(input);
  return wrap;
}

function checks(label, options, selected, onChange) {
  const wrap = document.createElement("div");
  const title = document.createElement("div");
  title.textContent = label;
  wrap.append(title);
  for (const option of options) {
    const row = document.createElement("label");
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = selected.includes(option);
    box.addEventListener("change", () => {
      const next = new Set(selected);
      if (box.checked) next.add(option);
      else next.delete(option);
      onChange([...next]);
      renderForm();
    });
    row.append(box, document.createTextNode(" " + option));
    wrap.append(row);
  }
  return wrap;
}

function lines(label, values, onChange) {
  return field(label, (values || []).join("\n"), (text) => {
    onChange(text.split("\n").map((s) => s.trim()).filter(Boolean));
  }, true);
}

function renderForm() {
  form.replaceChildren();
  const item = items()[state.index];
  if (!item) return;
  const set = (key) => (value) => { item[key] = value; };
  if (state.section === "stories") {
    form.append(
      field("Id", item.id, set("id")),
      field("Kicker", item.kicker, set("kicker")),
      field("Title", item.title, set("title")),
      field("Subtitle", item.subtitle, set("subtitle")),
      field("Body", item.body, set("body"), true),
      field("Image", item.image, set("image")),
      field("Color", item.color, set("color")),
      field("Date", item.date, set("date")),
    );
    const style = document.createElement("label");
    style.textContent = "Style";
    const select = document.createElement("select");
    for (const option of ["hero", "card"]) {
      const el = document.createElement("option");
      el.value = option;
      el.textContent = option;
      el.selected = item.style === option;
      select.append(el);
    }
    select.addEventListener("change", () => { item.style = select.value; });
    style.append(select);
    form.append(style);
    form.append(checks("Surfaces", window.bay.surfaces, item.surfaces || [], (v) => { item.surfaces = v; }));
    form.append(lines("Apps", item.apps, (v) => { item.apps = v; }));
    const image = document.createElement("button");
    image.type = "button";
    image.textContent = "Choose image";
    image.addEventListener("click", async () => {
      if (!state.dir) return;
      const picked = await window.bay.pickImage(state.dir);
      if (picked) {
        item.image = picked;
        renderForm();
      }
    });
    form.append(image);
  } else if (state.section === "rails") {
    form.append(
      field("Id", item.id, set("id")),
      field("Title", item.title, set("title")),
    );
    form.append(checks("Surface", window.bay.surfaces, item.surface ? [item.surface] : [], (v) => { item.surface = v[0] || ""; }));
    form.append(lines("Apps", item.apps, (v) => { item.apps = v; }));
  } else {
    form.append(
      field("Id", item.id, set("id")),
      field("Name", item.name, set("name")),
      field("Summary", item.summary, set("summary")),
      field("Developer", item.developer, set("developer")),
      field("Icon", item.icon, set("icon")),
      field("Download", item.download, set("download")),
      field("Version", item.version, set("version")),
      field("Homepage", item.homepage, set("homepage")),
    );
    form.append(checks("Categories", window.bay.categories, item.categories || [], (v) => { item.categories = v; }));
    const image = document.createElement("button");
    image.type = "button";
    image.textContent = "Choose icon";
    image.addEventListener("click", async () => {
      if (!state.dir) return;
      const picked = await window.bay.pickImage(state.dir);
      if (picked) {
        item.icon = picked;
        renderForm();
      }
    });
    form.append(image);
  }
  const remove = document.createElement("button");
  remove.type = "button";
  remove.textContent = "Remove";
  remove.addEventListener("click", () => {
    items().splice(state.index, 1);
    state.index = -1;
    renderList();
    renderForm();
  });
  form.append(remove);
}

document.querySelectorAll("nav button").forEach((button) => {
  button.addEventListener("click", () => {
    state.section = button.dataset.section;
    state.index = -1;
    renderList();
    renderForm();
  });
});

document.querySelector("#open").addEventListener("click", async () => {
  const opened = await window.bay.openDir();
  if (!opened) return;
  state.dir = opened.dir;
  state.catalog = opened.catalog;
  state.index = -1;
  dirLabel.textContent = opened.dir;
  showStatus("");
  renderList();
  renderForm();
});

document.querySelector("#save").addEventListener("click", async () => {
  if (!state.dir) {
    showStatus("Open a folder first.");
    return;
  }
  const result = await window.bay.save(state.dir, state.catalog);
  showStatus(result.ok ? "Saved." : result.errors.join(" "));
});

renderList();
