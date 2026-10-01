import { mount } from "svelte";
import App from "./App.svelte";
import { installDevIpcMock } from "./lib/devIpcMock";

import "./styles/styles-base.css";
import "./styles/styles-panel.css";
import "./styles/styles-effects.css";
import "./styles/styles-font.css";
import "./styles/styles-theme.css";

// Must run before the app is mounted: it flips `isTauriRuntime()` on, so the
// frontend takes its normal IPC path and `mockIPC` answers instead of the
// command hanging or rejecting against a browser with no backend.
installDevIpcMock();

const target = document.querySelector("#app");
const app = target ? mount(App, { target }) : null;

export default app;
