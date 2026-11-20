/**
 * Client-side diagram export. The whole diagram (not just the visible part)
 * is rendered from the canvas viewport element with `html-to-image`.
 */
import type { Box } from "@/lib/diagram";

export type ExportFormat = "png" | "svg" | "pdf";

const PADDING = 32;
/** Keeps PNGs well inside browser canvas limits. */
const MAX_PIXELS = 40_000_000;

export interface ExportFrame {
  width: number;
  height: number;
  /** CSS transform that moves the diagram's top-left corner into the frame. */
  transform: string;
}

/** Image size and viewport transform for a diagram's bounding box at 100% zoom. */
export function exportFrame(bounds: Box, padding = PADDING): ExportFrame {
  const width = Math.ceil(bounds.width + padding * 2);
  const height = Math.ceil(bounds.height + padding * 2);
  return {
    width,
    height,
    transform: `translate(${padding - bounds.x}px, ${padding - bounds.y}px) scale(1)`,
  };
}

/** 2x for sharp text, reduced for very large diagrams. */
export function pixelRatioFor(width: number, height: number): number {
  return Math.max(0.25, Math.min(2, Math.sqrt(MAX_PIXELS / Math.max(1, width * height))));
}

export function exportFileName(schema: string, format: ExportFormat): string {
  const safe = schema.replace(/[^\w.-]+/g, "_") || "diagram";
  return `${safe}-er-diagram.${format}`;
}

function download(url: string, fileName: string) {
  const a = document.createElement("a");
  a.href = url;
  a.download = fileName;
  document.body.appendChild(a);
  a.click();
  a.remove();
}

/** Prints an image through a hidden frame (no pop-up needed). */
function printImage(dataUrl: string, frame: ExportFrame, title: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const iframe = document.createElement("iframe");
    iframe.setAttribute("aria-hidden", "true");
    iframe.style.cssText = "position:fixed;width:0;height:0;border:0;right:0;bottom:0";
    document.body.appendChild(iframe);
    const doc = iframe.contentDocument;
    const win = iframe.contentWindow;
    if (!doc || !win) {
      iframe.remove();
      reject(new Error("Printing is not available in this browser."));
      return;
    }
    const landscape = frame.width >= frame.height;
    doc.open();
    doc.write(
      `<!doctype html><html><head><title>${title.replace(/[<&]/g, "")}</title><style>` +
        `@page { size: ${landscape ? "landscape" : "portrait"}; margin: 10mm; }` +
        "html, body { margin: 0; } img { display: block; max-width: 100%; max-height: 100vh; margin: auto; }" +
        `</style></head><body><img alt="" src="${dataUrl}"></body></html>`,
    );
    doc.close();
    const img = doc.querySelector("img")!;
    const go = () => {
      win.focus();
      win.print();
      // Give the print dialog time to read the frame before removing it.
      setTimeout(() => iframe.remove(), 60_000);
      resolve();
    };
    if (img.complete) go();
    else {
      img.onload = go;
      img.onerror = () => {
        iframe.remove();
        reject(new Error("The diagram image could not be prepared for printing."));
      };
    }
  });
}

/**
 * Exports the diagram. `viewport` is the canvas viewport element
 * (`.react-flow__viewport`), `bounds` the nodes' bounding box.
 */
export async function exportDiagram(options: {
  viewport: HTMLElement;
  bounds: Box;
  format: ExportFormat;
  schema: string;
  background: string;
}): Promise<void> {
  const { toPng, toSvg } = await import("html-to-image");
  const frame = exportFrame(options.bounds);
  const render = {
    backgroundColor: options.background,
    width: frame.width,
    height: frame.height,
    style: { width: `${frame.width}px`, height: `${frame.height}px`, transform: frame.transform },
  };
  const fileName = exportFileName(options.schema, options.format);
  switch (options.format) {
    case "png":
      download(await toPng(options.viewport, { ...render, pixelRatio: pixelRatioFor(frame.width, frame.height) }), fileName);
      return;
    case "svg":
      download(await toSvg(options.viewport, render), fileName);
      return;
    case "pdf":
      await printImage(await toSvg(options.viewport, render), frame, `${options.schema} ER diagram`);
      return;
  }
}
