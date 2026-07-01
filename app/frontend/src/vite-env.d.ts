/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_OTHERONE_WEB_API_BASE_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
