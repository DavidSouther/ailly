import type { Preview } from "@storybook/react-vite";

import "../src/styles/theme.css";
import { syncThemeWithSystem } from "../src/theme/systemTheme";

syncThemeWithSystem();

const preview: Preview = {
  parameters: {
    controls: {
      matchers: {
        color: /(background|color)$/i,
        date: /Date$/i,
      },
    },
  },
};

export default preview;
