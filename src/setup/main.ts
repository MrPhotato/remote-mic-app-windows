import { createApp } from "vue";
import SetupApp from "./SetupApp.vue";
import { followSystemTheme } from "./theme";
import { installFocusModalityTracking } from "../lib/focus-modality";
import "../styles.css";
import "./setup.css";

followSystemTheme();
installFocusModalityTracking();
createApp(SetupApp).mount("#app");
