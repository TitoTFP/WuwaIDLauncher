import { mount } from "svelte";
import App from "./App.svelte";

import "./styles/styles-base.css";
import "./styles/styles-panel.css";
import "./styles/styles-effects.css";
import "./styles/styles-font.css";
import "./styles/styles-theme.css";

const target = document.querySelector("#app");
const app = target ? mount(App, { target }) : null;

export default app;
