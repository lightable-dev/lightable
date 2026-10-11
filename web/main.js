// Loads the app (built by tools/build_web.sh). Everything runs in this
// page; files you drop are read here and never sent anywhere.
import init from "./lightable.js";

init().then(
  () => document.getElementById("loading")?.remove(),
  (e) => {
    const el = document.getElementById("loading");
    if (el) el.textContent = "Could not start: " + e;
  },
);
