# App Bay

The catalog window for Elavo. Flatpaks come from Flathub. Stories, featured rails, AppImages, and extra flatpakrefs come from a JSON catalog.

```sh
scripts/dev-run
```

That builds in the `elavo-dev` toolbox and opens the sample catalog in `catalog/sample`. `ELAVO_APP_BAY_CATALOG` can point at another directory, or at `https://elavo.io/app-bay`.

`editor/` is the Electron app that writes the catalog. It needs Node, which this machine's image does not include. From a machine that has it: `cd editor && npm install && npm start`.
