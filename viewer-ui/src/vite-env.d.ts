/// <reference types="vite/client" />

// Vite's `?inline` suffix returns an asset as a data-URI string rather than an
// emitted file URL. Used for the crow mark so it paints with the bundle instead
// of racing a separate fetch on the loading splash.
declare module "*.svg?inline" {
  const src: string;
  export default src;
}
