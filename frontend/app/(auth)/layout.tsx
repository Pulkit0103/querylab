export default function AuthLayout({ children }: LayoutProps<"/">) {
  return (
    <div className="grid h-dvh lg:grid-cols-[1fr_1.1fr]">
      <main className="flex items-center justify-center overflow-y-auto p-6">{children}</main>
      <aside className="relative hidden overflow-hidden border-l bg-panel lg:block" aria-hidden>
        <div
          className="absolute inset-0"
          style={{
            background:
              "radial-gradient(ellipse at top left, color-mix(in oklch, var(--brand) 9%, transparent), transparent 60%)",
          }}
        />
        <div className="relative flex h-full flex-col justify-center p-12">
          <pre className="max-w-lg rounded-xl border bg-background/60 p-6 font-mono text-[13px] leading-6 text-muted-foreground shadow-2xl">
            <span className="text-brand">SELECT</span> c.country,{"\n"}
            {"       "}
            <span className="text-brand">count</span>(*) <span className="text-brand">AS</span> orders,{"\n"}
            {"       "}
            <span className="text-brand">sum</span>(o.total) <span className="text-brand">AS</span> revenue{"\n"}
            <span className="text-brand">FROM</span> sales.orders o{"\n"}
            <span className="text-brand">JOIN</span> sales.customers c <span className="text-brand">ON</span> c.id = o.customer_id{"\n"}
            <span className="text-brand">GROUP BY</span> 1{"\n"}
            <span className="text-brand">ORDER BY</span> revenue <span className="text-brand">DESC</span>;
          </pre>
          <p className="mt-8 max-w-md text-sm text-muted-foreground">
            Write SQL with autocomplete, explore schemas, inspect execution plans — with credentials encrypted at rest and
            read-only execution by default.
          </p>
        </div>
      </aside>
    </div>
  );
}
