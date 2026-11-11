import Link from "next/link";

export default function NotFound() {
  return (
    <main className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
      <p className="font-mono text-sm text-muted-foreground">404</p>
      <h1 className="text-lg font-semibold">Page not found</h1>
      <Link href="/editor" className="text-sm text-brand hover:underline">
        Back to the editor
      </Link>
    </main>
  );
}
