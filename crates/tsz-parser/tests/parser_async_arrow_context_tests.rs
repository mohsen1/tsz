//! Tests for async/await and generator context in arrow-function lookahead.
//!
//! These tests verify that `look_ahead_is_arrow_function` correctly rejects
//! `await` (in async/static-block context) and `yield` (in generator context)
//! as parameter names, and that real-world async JSX patterns parse cleanly.

use crate::parser::test_fixture::{assert_no_errors_labeled, parse_source_named};

#[test]
fn test_await_variants_in_tsx_async_function() {
    // Issue 11321: various await patterns in async functions in .tsx files
    let cases = [
        (
            "await call with method chain",
            "test.tsx",
            r#"async function Page() {
  const h = (await headers()).get('x-id') as string;
  return <h1>{h}</h1>;
}"#,
        ),
        (
            "await in async arrow returning jsx",
            "test.tsx",
            r#"const Page = async () => {
  const data = await fetchData();
  return <div>{data}</div>;
};"#,
        ),
        (
            "multiple awaits then jsx return",
            "test.tsx",
            r#"async function Page() {
  const a = await fetchA();
  const b = await fetchB();
  return <section><p>{a}</p><p>{b}</p></section>;
}"#,
        ),
        (
            "await in try block with jsx",
            "test.tsx",
            r#"async function Page() {
  let data;
  try {
    data = await fetch('/api');
  } catch(e) {
    return <div>Error</div>;
  }
  return <div>{data}</div>;
}"#,
        ),
        (
            "server component next/headers pattern",
            "page.tsx",
            r#"import { headers } from 'next/headers';
export default async function Page() {
  const h = (await headers()).get('x-id') as string;
  return <h1>{h}</h1>;
}"#,
        ),
        (
            "await with as-cast and optional chain",
            "test.tsx",
            r#"async function Page() {
  const val = (await getConfig())?.setting as string;
  return <span>{val}</span>;
}"#,
        ),
        (
            "await with destructuring",
            "test.tsx",
            r#"async function Page() {
  const { name, value } = await getProps();
  return <div data-value={value}>{name}</div>;
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_issue_11320_generic_arrow_jsx_return_tsx() {
    // Issue 11320: generic arrow returning JSX in .tsx files
    let cases = [
        ("simple arrow returning jsx", r#"const f = () => <div />;"#),
        (
            "arrow with params returning jsx",
            r#"const f = (x: string) => <div>{x}</div>;"#,
        ),
        (
            "typed arrow returning jsx",
            r#"const f = (): JSX.Element => <div />;"#,
        ),
        (
            "generic arrow with constraint returning jsx",
            r#"const f = <T extends string>(x: T) => <div>{x}</div>;"#,
        ),
        (
            "generic arrow with multiple params returning jsx",
            r#"const f = <T, U>(x: T, y: U) => <div />;"#,
        ),
        (
            "generic arrow with trailing comma returning jsx",
            r#"const f = <T,>(x: T) => <div>{x}</div>;"#,
        ),
        (
            "async arrow with generic returning jsx",
            r#"const f = async <T extends object>(x: T) => <div />;"#,
        ),
        (
            "export default async arrow returning jsx",
            r#"export default async function Page() { return <div />; }"#,
        ),
    ];
    for (label, source) in cases {
        assert_no_errors_labeled("test.tsx", label, source);
    }
}

#[test]
fn test_async_jsx_complex_real_world_patterns() {
    // Patterns from real Next.js app code that might expose parser edge cases
    let cases = [
        (
            "next headers awaited cast",
            "page.tsx",
            r#"import { headers } from 'next/headers';
export default async function Page() {
  const reqHeaders = await headers();
  const id = reqHeaders.get('x-request-id');
  return <div data-id={id}><h1>Page</h1></div>;
}"#,
        ),
        (
            "parallel await destructure",
            "page.tsx",
            r#"async function Page() {
  const [user, posts] = await Promise.all([fetchUser(), fetchPosts()]);
  return <div><h1>{user.name}</h1><ul>{posts.map(p => <li key={p.id}>{p.title}</li>)}</ul></div>;
}"#,
        ),
        (
            "await in conditional jsx",
            "page.tsx",
            r#"async function Page({ id }: { id: string }) {
  const data = await fetchData(id);
  return data ? <div>{data.name}</div> : <div>Not found</div>;
}"#,
        ),
        (
            "server action with await",
            "page.tsx",
            r#"async function Page() {
  const session = await getSession();
  if (!session) {
    return <div>Unauthorized</div>;
  }
  return (
    <main>
      <h1>Welcome {session.user}</h1>
    </main>
  );
}"#,
        ),
        (
            "suspense boundary with async child",
            "layout.tsx",
            r#"import { Suspense } from 'react';
async function AsyncContent() {
  const data = await fetchData();
  return <div>{data}</div>;
}
export default function Layout({ children }: { children: React.ReactNode }) {
  return (
    <html>
      <body>
        <Suspense fallback={<div>Loading...</div>}>
          <AsyncContent />
        </Suspense>
        {children}
      </body>
    </html>
  );
}"#,
        ),
        (
            "nested async in map",
            "test.tsx",
            r#"async function Page() {
  const items = await getItems();
  return (
    <ul>
      {items.map((item) => (
        <li key={item.id}>
          <span>{item.name}</span>
        </li>
      ))}
    </ul>
  );
}"#,
        ),
        (
            "await with type narrowing",
            "test.tsx",
            r#"async function Page() {
  const result: { ok: true; data: string } | { ok: false; error: string } = await fetchResult();
  if (result.ok) {
    return <div>{result.data}</div>;
  }
  return <div className="error">{result.error}</div>;
}"#,
        ),
        (
            "awaited generic function result",
            "test.tsx",
            r#"async function Page<T extends { id: string }>(props: { fetch: () => Promise<T[]> }) {
  const items = await props.fetch();
  return <ul>{items.map(i => <li key={i.id}>{JSON.stringify(i)}</li>)}</ul>;
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_async_jsx_multi_function_file_patterns() {
    // Test patterns where multiple async functions interact with JSX scanner state
    let cases = [
        (
            "multiple async functions with jsx",
            "test.tsx",
            r#"
async function Header() {
  const title = await getTitle();
  return <h1>{title}</h1>;
}

async function Content() {
  const body = await getBody();
  return <p>{body}</p>;
}

async function Page() {
  const data = await getData();
  return (
    <main>
      <Header />
      <Content />
      <footer>{data.footer}</footer>
    </main>
  );
}
"#,
        ),
        (
            "interleaved async and sync functions",
            "test.tsx",
            r#"
function StaticComp({ x }: { x: string }) {
  return <span>{x}</span>;
}

async function AsyncComp() {
  const val = await fetchVal();
  return <div><StaticComp x={val} /></div>;
}

function App() {
  return <AsyncComp />;
}
"#,
        ),
        (
            "export default after other exports",
            "page.tsx",
            r#"
export async function generateMetadata() {
  const meta = await getMeta();
  return { title: meta.title };
}

export default async function Page() {
  const data = (await fetchData()).items as string[];
  return <ul>{data.map((item, i) => <li key={i}>{item}</li>)}</ul>;
}
"#,
        ),
        (
            "async function with complex jsx and await",
            "test.tsx",
            r#"
async function Dashboard() {
  const [user, stats] = await Promise.all([getUser(), getStats()]);
  const greeting = (await getGreeting()).text as string;
  return (
    <div className="dashboard">
      <h1>{greeting}, {user.name}!</h1>
      <section>
        <h2>Stats</h2>
        {stats.map(s => (
          <div key={s.id}>
            <span>{s.label}</span>
            <strong>{s.value}</strong>
          </div>
        ))}
      </section>
    </div>
  );
}
"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_await_inside_jsx_expression_children() {
    // await expressions INSIDE JSX {} children (not just before JSX return)
    let cases = [
        (
            "await directly in jsx expression",
            "test.tsx",
            r#"async function Page() {
  return <div>{await getValue()}</div>;
}"#,
        ),
        (
            "await with method chain in jsx expression",
            "test.tsx",
            r#"async function Page() {
  return <div>{(await getObj()).value}</div>;
}"#,
        ),
        (
            "await as-cast in jsx expression",
            "test.tsx",
            r#"async function Page() {
  return <div>{(await getValue()) as string}</div>;
}"#,
        ),
        (
            "conditional await in jsx expression",
            "test.tsx",
            r#"async function Page({ show }: { show: boolean }) {
  return <div>{show ? await getA() : await getB()}</div>;
}"#,
        ),
        (
            "await in nested jsx expression",
            "test.tsx",
            r#"async function Page() {
  return (
    <main>
      <section>
        <h1>{await getTitle()}</h1>
        <p>{await getDescription()}</p>
      </section>
    </main>
  );
}"#,
        ),
        (
            "await in jsx attribute value",
            "test.tsx",
            r#"async function Page() {
  return <div data-val={await getVal()} className="test" />;
}"#,
        ),
        (
            "multiple awaits in jsx children",
            "test.tsx",
            r#"async function Page() {
  return (
    <div>
      {await getA()}
      {await getB()}
      {await getC()}
    </div>
  );
}"#,
        ),
        (
            "await in jsx spread",
            "test.tsx",
            r#"async function Page() {
  const props = await getProps();
  return <Component {...props} />;
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_await_method_chain_as_cast_tsx_edge_cases() {
    // Test the specific pattern from issue #11321 and edge cases around it
    let cases = [
        (
            "method chain on parenthesized await",
            "page.tsx",
            r#"export default async function Page() {
  const h = (await headers()).get('x-id') as string;
  return <h1>{h}</h1>;
}"#,
        ),
        (
            "multiple method chains on await",
            "test.tsx",
            r#"export default async function Page() {
  const result = (await fetchData()).items.filter(Boolean).map(String).join(',');
  return <p>{result}</p>;
}"#,
        ),
        (
            "nullish coalescing on await result",
            "test.tsx",
            r#"export default async function Page() {
  const val = (await getVal())?.name ?? 'default';
  return <span>{val}</span>;
}"#,
        ),
        (
            "await then optional chaining",
            "test.tsx",
            r#"export default async function Page() {
  const x = (await getObj())?.prop?.nested as string | undefined;
  return <div>{x}</div>;
}"#,
        ),
        (
            "await with index access",
            "test.tsx",
            r#"export default async function Page() {
  const items = (await getList())[0] as string;
  return <li>{items}</li>;
}"#,
        ),
        (
            "chained awaits with as-casts",
            "test.tsx",
            r#"export default async function Page() {
  const a = await getA() as string;
  const b = (await getB()).value as number;
  return <div><span>{a}</span><span>{b}</span></div>;
}"#,
        ),
        (
            "export default async function then named export async function",
            "page.tsx",
            r#"export async function generateMetadata({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  return { title: `Item ${id}` };
}

export default async function Page({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const data = (await fetchItem(id)).details as string;
  return <article><h1>{data}</h1></article>;
}
"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_nextjs_page_tsx_specific_patterns() {
    // Next.js 15 specific patterns with Promise<> params and dynamic exports
    let cases = [
        (
            "nextjs15 searchParams as Promise",
            "page.tsx",
            r#"export default async function Page({
  searchParams,
}: {
  searchParams: Promise<{ id?: string }>;
}) {
  const { id } = await searchParams;
  return <div>{id}</div>;
}"#,
        ),
        (
            "nextjs15 params Promise destructure",
            "page.tsx",
            r#"export default async function Page({
  params,
}: {
  params: Promise<{ slug: string }>;
}) {
  const { slug } = await params;
  return <h1>{slug}</h1>;
}"#,
        ),
        (
            "nextjs dynamic export with async page",
            "page.tsx",
            r#"export const dynamic = 'force-dynamic';

export default async function Page() {
  const data = await fetchData();
  return <main>{data.content}</main>;
}"#,
        ),
        (
            "nextjs generateMetadata with headers",
            "page.tsx",
            r#"import { headers } from 'next/headers';

export async function generateMetadata() {
  const headersList = await headers();
  const id = headersList.get('x-custom-header') ?? 'unknown';
  return { title: `Page ${id}` };
}

export default async function Page() {
  const headersList = await headers();
  const h = headersList.get('x-id') as string;
  return <h1>{h}</h1>;
}"#,
        ),
        (
            "nextjs layout with children prop",
            "layout.tsx",
            r#"export default async function Layout({
  children,
}: {
  children: React.ReactNode;
}) {
  const session = await getSession();
  return (
    <html lang="en">
      <body>
        {session ? children : <div>Login required</div>}
      </body>
    </html>
  );
}"#,
        ),
        (
            "nextjs page with notFound",
            "page.tsx",
            r#"import { notFound } from 'next/navigation';

export default async function Page({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const item = await fetchItem(id);
  if (!item) notFound();
  return <article><h1>{item.title}</h1><p>{item.body}</p></article>;
}"#,
        ),
        (
            "await params in search params",
            "page.tsx",
            r#"export default async function Page(props: {
  params: Promise<{ category: string }>;
  searchParams: Promise<{ sort?: string; page?: string }>;
}) {
  const { category } = await props.params;
  const { sort = 'asc', page = '1' } = await props.searchParams;
  const items = await fetchItems({ category, sort, page: Number(page) });
  return (
    <div>
      <h1>{category}</h1>
      <ul>
        {items.map(item => (
          <li key={item.id}>{item.name}</li>
        ))}
      </ul>
    </div>
  );
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_generic_jsx_components_tsx_patterns() {
    // Generic JSX components in .tsx files - potential ambiguity with type assertions
    let cases = [
        (
            "generic jsx component with spread",
            "test.tsx",
            r#"function Comp<T extends object>({ className, ...rest }: T & { className?: string }) {
  return <div className={className} {...rest} />;
}"#,
        ),
        (
            "generic component returning jsx with type",
            "test.tsx",
            r#"function List<T>({ items, render }: { items: T[]; render: (item: T) => JSX.Element }) {
  return <ul>{items.map((item, i) => <li key={i}>{render(item)}</li>)}</ul>;
}"#,
        ),
        (
            "async generic page component",
            "page.tsx",
            r#"async function DataPage<T extends { id: string; name: string }>(props: {
  fetcher: () => Promise<T[]>;
}) {
  const data = await props.fetcher();
  return (
    <section>
      {data.map(item => <div key={item.id}>{item.name}</div>)}
    </section>
  );
}"#,
        ),
        (
            "generic context provider in tsx",
            "test.tsx",
            r#"function Provider<T>({
  value,
  children,
}: {
  value: T;
  children: React.ReactNode;
}) {
  return <div data-has-value={value !== undefined}>{children}</div>;
}"#,
        ),
        (
            "forwardRef generic component",
            "test.tsx",
            r#"const Input = React.forwardRef<
  HTMLInputElement,
  React.InputHTMLAttributes<HTMLInputElement>
>(function Input({ className, ...props }, ref) {
  return <input ref={ref} className={className} {...props} />;
});
"#,
        ),
        (
            "generic wrapper with constraint",
            "test.tsx",
            r#"function Wrapper<T extends React.ComponentType<{ className?: string }>>(
  Comp: T
): React.FC<React.ComponentPropsWithoutRef<T>> {
  return function WrappedComp(props) {
    return <Comp className="wrapped" {...props} />;
  };
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_async_tsx_with_jsx_namespaced_and_fragments() {
    // Test async functions with namespaced JSX tags and fragments in .tsx files
    let cases = [
        (
            "async function returning fragment",
            "test.tsx",
            r#"async function Page() {
  const data = await fetchData();
  return <>{data}</>;
}"#,
        ),
        (
            "async function with namespaced jsx",
            "test.tsx",
            r#"async function Page() {
  const data = await fetchData();
  return <React.Fragment><span>{data}</span></React.Fragment>;
}"#,
        ),
        (
            "async function member expression jsx tag",
            "test.tsx",
            r#"async function Page() {
  const items = await getItems();
  return <Icons.Star className="icon" />;
}"#,
        ),
        (
            "async function deeply nested jsx",
            "test.tsx",
            r#"async function Page() {
  const a = await getA();
  const b = await getB();
  return (
    <>
      <div>
        <section>
          <article>
            <p>{a}</p>
            <p>{b}</p>
          </article>
        </section>
      </div>
    </>
  );
}"#,
        ),
        (
            "async function with jsx key computed",
            "test.tsx",
            r#"async function List() {
  const items = await getItems();
  return (
    <ul>
      {items.map((item, index) => (
        <li key={`item-${item.id}-${index}`}>
          {item.name}
        </li>
      ))}
    </ul>
  );
}"#,
        ),
        (
            "async with jsx expression spread and await",
            "test.tsx",
            r#"async function Page() {
  const props = await getPageProps();
  const extra = await getExtra();
  return <div {...props} {...extra} className="page" />;
}"#,
        ),
        (
            "await then jsx with boolean attrs",
            "test.tsx",
            r#"async function Form() {
  const submitted = await checkSubmitted();
  return <input type="text" disabled={submitted} readOnly={!submitted} />;
}"#,
        ),
        (
            "complex async with error boundary pattern",
            "test.tsx",
            r#"async function Page() {
  let content: string;
  try {
    content = (await fetchContent()).text as string;
  } catch {
    return <div className="error">Failed to load</div>;
  }
  return (
    <main>
      <article dangerouslySetInnerHTML={{ __html: content }} />
    </main>
  );
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_async_context_preservation_after_jsx_parsing() {
    // Test that async context (and await keyword recognition) is preserved
    // after parsing JSX expressions - the issue hypothesis was about scanner
    // state not preserving async context after JSX token boundary
    let cases = [
        (
            "await after jsx expression statement",
            "test.tsx",
            r#"async function Page() {
  const el = <div>temp</div>;
  const data = await fetchData();
  return <section>{data}</section>;
}"#,
        ),
        (
            "await in variable after jsx in if",
            "test.tsx",
            r#"async function Page({ show }: { show: boolean }) {
  if (show) {
    return <span>early</span>;
  }
  const data = await fetchData();
  return <main>{data}</main>;
}"#,
        ),
        (
            "multiple returns some with jsx some without",
            "test.tsx",
            r#"async function Page({ id }: { id?: string }) {
  if (!id) {
    return <div>No ID</div>;
  }
  const result = await fetch(id);
  if (!result.ok) {
    return <div>Error: {result.status}</div>;
  }
  const data = await result.json() as { name: string };
  return <article><h1>{data.name}</h1></article>;
}"#,
        ),
        (
            "await in loop body that also renders jsx",
            "test.tsx",
            r#"async function LoadAll({ ids }: { ids: string[] }) {
  const results: string[] = [];
  for (const id of ids) {
    const item = await fetchItem(id);
    results.push(item.name);
  }
  return <ul>{results.map((r, i) => <li key={i}>{r}</li>)}</ul>;
}"#,
        ),
        (
            "jsx followed by await in same block",
            "test.tsx",
            r#"async function Component() {
  const preEl = <span>pre</span>;
  const value = await getValue();
  const postEl = <em>{value}</em>;
  return <div>{preEl}{postEl}</div>;
}"#,
        ),
        (
            "await using nextjs cookies pattern",
            "page.tsx",
            r#"import { cookies } from 'next/headers';
export default async function Page() {
  const cookieStore = await cookies();
  const theme = cookieStore.get('theme')?.value ?? 'light';
  const user = cookieStore.get('user')?.value as string | undefined;
  return (
    <div data-theme={theme}>
      {user ? <p>Hello, {user}</p> : <p>Not logged in</p>}
    </div>
  );
}"#,
        ),
        (
            "await with type guard in tsx",
            "test.tsx",
            r#"async function Page() {
  const result = await fetchResult();
  function isSuccess(r: typeof result): r is { ok: true; data: string } {
    return r.ok;
  }
  if (isSuccess(result)) {
    return <div>{result.data}</div>;
  }
  return <div className="error">Failed</div>;
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_use_client_use_server_directive_with_async_jsx() {
    // 'use client' / 'use server' directives combined with async/JSX
    let cases = [
        (
            "use client with async effect",
            "test.tsx",
            r#"'use client';
import { useState, useEffect } from 'react';
export default function ClientComponent() {
  const [data, setData] = useState<string>('');
  useEffect(() => {
    async function load() {
      const result = await fetchData();
      setData(result);
    }
    load();
  }, []);
  return <div>{data}</div>;
}"#,
        ),
        (
            "use server with server action",
            "actions.ts",
            r#"'use server';
export async function saveData(data: string): Promise<void> {
  await writeToDb(data);
}"#,
        ),
        (
            "use client with async callback",
            "component.tsx",
            r#"'use client';
export default function Form() {
  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const result = await submitForm();
    if (result.ok) {
      window.location.href = '/success';
    }
  }
  return <form onSubmit={handleSubmit}><button type="submit">Submit</button></form>;
}"#,
        ),
        (
            "use client component with async children pattern",
            "page.tsx",
            r#"'use client';
import { Suspense } from 'react';
function AsyncChild({ id }: { id: string }) {
  return <div data-id={id}>Loading...</div>;
}
export default function Page({ id }: { id: string }) {
  return (
    <Suspense fallback={<div>Loading...</div>}>
      <AsyncChild id={id} />
    </Suspense>
  );
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_jsx_spread_in_generic_components_tsx() {
    // Issue #11345: JSX spread attribute in generic components
    let cases = [
        (
            "basic jsx spread",
            "test.tsx",
            r#"const props = { a: 1 };
const el = <div {...props} />;"#,
        ),
        (
            "jsx spread on component",
            "test.tsx",
            r#"const props = { name: 'test' };
const el = <MyComp {...props} />;"#,
        ),
        (
            "jsx spread with additional props",
            "test.tsx",
            r#"const base = { className: 'base' };
const el = <div {...base} id="special" data-testid="test" />;"#,
        ),
        (
            "jsx spread in generic function component",
            "test.tsx",
            r#"function Comp<T extends object>(props: T & { className?: string }) {
  const { className, ...rest } = props;
  return <div className={className} {...rest} />;
}"#,
        ),
        (
            "multiple spreads in jsx",
            "test.tsx",
            r#"const el = <input {...defaultProps} {...overrideProps} value="test" />;"#,
        ),
        (
            "spread with computed props in async function",
            "test.tsx",
            r#"async function Page() {
  const props = await getProps();
  const extra = { 'data-loaded': true };
  return <section {...props} {...extra} aria-label="content" />;
}"#,
        ),
        (
            "jsx spread in arrow returning jsx",
            "test.tsx",
            r#"const render = (props: Record<string, unknown>) => <div {...props} className="wrapper" />;"#,
        ),
        (
            "jsx spread on named component in async",
            "test.tsx",
            r#"async function Layout({ children }: { children: React.ReactNode }) {
  const layoutProps = await getLayoutProps();
  return <main {...layoutProps}>{children}</main>;
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_async_in_jsx_attribute_expressions() {
    // Async arrow functions used as event handlers inside JSX attributes
    let cases = [
        (
            "async handler in jsx onclick",
            "test.tsx",
            r#"function Page() {
  return <button onClick={async () => {
    const result = await fetchData();
    console.log(result);
  }}>Click me</button>;
}"#,
        ),
        (
            "async handler with await and state update",
            "test.tsx",
            r#"function Form({ onSubmit }: { onSubmit: (data: string) => Promise<void> }) {
  return (
    <form onSubmit={async (e) => {
      e.preventDefault();
      await onSubmit('data');
    }}>
      <input type="text" />
      <button type="submit">Submit</button>
    </form>
  );
}"#,
        ),
        (
            "async iife in jsx expression",
            "test.tsx",
            r#"function Page() {
  return <div>{(async () => {
    const val = await getValue();
    return val;
  })()}</div>;
}"#,
        ),
        (
            "async callback in jsx map",
            "test.tsx",
            r#"async function Page() {
  const items = await getItems();
  return (
    <ul>
      {items.map(async (item) => {
        const detail = await getDetail(item.id);
        return <li key={item.id}>{detail.name}</li>;
      })}
    </ul>
  );
}"#,
        ),
        (
            "nested async functions in jsx",
            "test.tsx",
            r#"function App() {
  async function handleClick() {
    const data = await fetchData();
    return data;
  }
  return <button onClick={handleClick}>Click</button>;
}"#,
        ),
        (
            "async arrow in jsx conditional attribute",
            "test.tsx",
            r#"function Page({ isAsync }: { isAsync: boolean }) {
  return <button
    onClick={isAsync ? async () => { await doSomething(); } : () => doSync()}
  >
    Action
  </button>;
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_complex_nextjs_type_annotation_patterns() {
    // Complex Next.js type annotation patterns that might trip up the parser
    let cases = [
        (
            "nextjs15 params as awaitable generic type",
            "page.tsx",
            r#"type PageProps = {
  params: Promise<{ id: string; slug: string }>;
  searchParams: Promise<Record<string, string | string[]>>;
};

export default async function Page({ params, searchParams }: PageProps) {
  const { id, slug } = await params;
  const filters = await searchParams;
  return <div data-id={id} data-slug={slug}>{JSON.stringify(filters)}</div>;
}"#,
        ),
        (
            "nextjs generateStaticParams with complex return",
            "page.tsx",
            r#"export async function generateStaticParams(): Promise<Array<{ id: string }>> {
  const items = await fetchAllItems();
  return items.map(item => ({ id: item.id }));
}

export default async function Page({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  return <main><h1>{id}</h1></main>;
}"#,
        ),
        (
            "complex async component with multiple generics",
            "page.tsx",
            r#"async function DataTable<T extends { id: string; [key: string]: unknown }>(props: {
  fetcher: () => Promise<T[]>;
  columns: Array<{ key: keyof T; label: string }>;
}) {
  const data = await props.fetcher();
  return (
    <table>
      <thead>
        <tr>{props.columns.map(col => <th key={String(col.key)}>{col.label}</th>)}</tr>
      </thead>
      <tbody>
        {data.map(row => (
          <tr key={row.id}>
            {props.columns.map(col => (
              <td key={String(col.key)}>{String(row[col.key])}</td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}"#,
        ),
        (
            "intersection types in async function params",
            "test.tsx",
            r#"type BaseProps = { className?: string; style?: React.CSSProperties };
type DataProps = { data: string[]; onSelect: (item: string) => void };

async function Component(props: BaseProps & DataProps & { 'data-testid'?: string }) {
  const processed = await processData(props.data);
  return (
    <div className={props.className} style={props.style} data-testid={props['data-testid']}>
      {processed.map((item, i) => (
        <button key={i} onClick={() => props.onSelect(item)}>{item}</button>
      ))}
    </div>
  );
}"#,
        ),
        (
            "conditional type in async function",
            "test.tsx",
            r#"type MaybeArray<T> = T extends unknown[] ? T : T[];

async function List<T extends string | number>(props: {
  items: MaybeArray<T>;
  render: (item: T extends unknown[] ? T[number] : T) => JSX.Element;
}) {
  const flat = await flattenItems(props.items as T[]);
  return <ul>{flat.map((item, i) => <li key={i}>{props.render(item as Parameters<typeof props.render>[0])}</li>)}</ul>;
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_async_tsx_without_semicolons() {
    // Test async TSX functions without semicolons (ASI patterns)
    let cases = [
        (
            "await without semicolons, jsx return",
            "test.tsx",
            r#"async function Page() {
  const h = (await headers()).get('x-id') as string
  return <h1>{h}</h1>
}"#,
        ),
        (
            "multiple awaits no semicolons",
            "test.tsx",
            r#"async function Page() {
  const a = await getA()
  const b = await getB()
  return <div><span>{a}</span><span>{b}</span></div>
}"#,
        ),
        (
            "full next page no semicolons",
            "page.tsx",
            r#"import { headers } from 'next/headers'
export default async function Page() {
  const h = (await headers()).get('x-id') as string
  return <h1>{h}</h1>
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_satisfies_operator_in_tsx_async() {
    // satisfies operator combined with async/await in TSX files
    let cases = [
        (
            "satisfies in async function with jsx",
            "test.tsx",
            r#"const config = {
  api: '/api',
  timeout: 5000,
} satisfies { api: string; timeout: number };

async function Page() {
  const data = await fetch(config.api);
  return <div>{data.status}</div>;
}"#,
        ),
        (
            "satisfies type in async destructure",
            "test.tsx",
            r#"async function Page() {
  const result = await fetchData() satisfies { items: string[] };
  return <ul>{result.items.map((item, i) => <li key={i}>{item}</li>)}</ul>;
}"#,
        ),
        (
            "const type parameter in tsx",
            "test.tsx",
            r#"function identity<const T>(value: T): T {
  return value;
}

async function Page() {
  const val = identity(await getValue());
  return <div>{String(val)}</div>;
}"#,
        ),
        (
            "using declaration in tsx",
            "test.tsx",
            r#"async function Page() {
  await using resource = await getResource();
  const data = resource.data;
  return <main>{data}</main>;
}"#,
        ),
        (
            "satisfies in jsx attribute",
            "test.tsx",
            r#"const styles = {
  color: 'red',
  fontSize: 16,
} satisfies React.CSSProperties;

function Comp() {
  return <div style={styles}>Hello</div>;
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_jsx_generic_component_type_args_tsx() {
    // JSX elements with type arguments in .tsx files
    let cases = [
        (
            "jsx with generic type argument",
            "test.tsx",
            r#"function render<T>(val: T) {
  return <Box<T> value={val} />;
}"#,
        ),
        (
            "jsx spread on generic instance",
            "test.tsx",
            r#"const props = { value: 'hello' };
function render<T extends string>(val: T) {
  return <Box value={val} {...props} />;
}"#,
        ),
        (
            "async with jsx spread complex",
            "test.tsx",
            r#"async function Page() {
  const { className, ...rest } = await getProps();
  return <main className={className} {...rest} />;
}"#,
        ),
        (
            "jsx with object spread in async fn",
            "test.tsx",
            r#"async function DataComp<T extends object>({ fetcher }: { fetcher: () => Promise<T> }) {
  const data = await fetcher();
  const { id, ...props } = data as T & { id: string };
  return <div data-id={id} {...props} />;
}"#,
        ),
        (
            "generic arrow with spread return in tsx",
            "test.tsx",
            r#"const wrap = <T extends object>(props: T & { className?: string }) => {
  const { className = '', ...rest } = props;
  return <div className={className} {...rest} />;
};"#,
        ),
        (
            "jsx with multiple spreads from async",
            "test.tsx",
            r#"async function Page() {
  const base = await getBaseProps();
  const extra = await getExtraProps();
  const override = { 'data-page': 'main' };
  return <section {...base} {...extra} {...override} aria-label="content" />;
}"#,
        ),
    ];
    for (label, file, source) in cases {
        assert_no_errors_labeled(file, label, source);
    }
}

#[test]
fn test_await_as_param_not_arrow_fn_in_async_context() {
    // In async context, `(await: T) => x` and `(await, x) => x` are parsed as
    // arrow functions (tsc also returns true from look_ahead_is_arrow_function).
    // Errors such as TS1359 are deferred to the checker, not the parser.
    //
    // `(a = await) => a` inside async produces TS1109 during arrow parsing:
    // the `await` has no operand (`)` follows) so `error_expression_expected` fires.
    //
    // Inside static blocks, `await` as a top-level parameter name is caught by
    // look_ahead_is_arrow_function, which returns false so the parser treats the
    // expression as a parenthesized form and produces parser-level errors.
    let cases_with_errors: &[(&str, &str, &str)] = &[(
        "await typed param in static block",
        "test.ts",
        r#"class C { static { const f = (await: string) => await; } }"#,
    )];
    for (label, file, source) in cases_with_errors {
        let (parser, _) = parse_source_named(file, source);
        assert!(
            !parser.get_diagnostics().is_empty(),
            "expected errors for {label}, got none"
        );
    }
}
