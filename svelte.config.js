import preprocess from "svelte-preprocess";
import staticAdapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/kit/vite";

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: [
    vitePreprocess(),
    preprocess({
      postcss: true,
    }),
  ],
  kit: {
    adapter: staticAdapter(),
  },
};

export default config;
