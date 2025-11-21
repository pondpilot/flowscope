/**
 * Type definitions for html-to-image
 * This provides TypeScript type safety for the html-to-image library.
 */

declare module 'html-to-image' {
  export interface Options {
    /**
     * Width of the output image in pixels
     */
    width?: number;

    /**
     * Height of the output image in pixels
     */
    height?: number;

    /**
     * Background color for the image
     */
    backgroundColor?: string;

    /**
     * Pixel ratio for high-DPI displays
     */
    pixelRatio?: number;

    /**
     * Quality of the image (0-1) for JPEG
     */
    quality?: number;

    /**
     * CSS font embed options
     */
    fontEmbedCSS?: string;

    /**
     * Skip fonts embedding
     */
    skipFonts?: boolean;

    /**
     * Function to filter nodes
     */
    filter?: (node: HTMLElement) => boolean;

    /**
     * Cache busting
     */
    cacheBust?: boolean;

    /**
     * Image timeout in milliseconds
     */
    imagePlaceholder?: string;

    /**
     * Whether to include data URLs for images
     */
    includeQueryParams?: boolean;
  }

  /**
   * Converts a DOM node to a PNG data URL
   */
  export function toPng(node: HTMLElement, options?: Options): Promise<string>;

  /**
   * Converts a DOM node to a JPEG data URL
   */
  export function toJpeg(node: HTMLElement, options?: Options): Promise<string>;

  /**
   * Converts a DOM node to an SVG data URL
   */
  export function toSvg(node: HTMLElement, options?: Options): Promise<string>;

  /**
   * Converts a DOM node to a Blob
   */
  export function toBlob(node: HTMLElement, options?: Options): Promise<Blob>;

  /**
   * Converts a DOM node to a canvas
   */
  export function toCanvas(node: HTMLElement, options?: Options): Promise<HTMLCanvasElement>;

  /**
   * Converts a DOM node to a pixel array
   */
  export function toPixelData(node: HTMLElement, options?: Options): Promise<Uint8ClampedArray>;
}
