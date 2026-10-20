// A static server for the built book (book/book), for the Playwright suite.
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../../../book/book/", import.meta.url));

Bun.serve({
  port: 5180,
  async fetch(request) {
    const path = decodeURIComponent(new URL(request.url).pathname);
    const file = Bun.file(root + (path.endsWith("/") ? `${path}index.html` : path).replace(/^\/+/, ""));
    return (await file.exists()) ? new Response(file) : new Response("not found", { status: 404 });
  },
});
