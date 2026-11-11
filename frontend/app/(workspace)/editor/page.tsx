import type { Metadata } from "next";

import { EditorLoader } from "./editor-loader";

export const metadata: Metadata = { title: "SQL Editor" };

export default function EditorPage() {
  return <EditorLoader />;
}
