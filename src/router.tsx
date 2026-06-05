import {
  createRootRoute,
  createRoute,
  createRouter,
  lazyRouteComponent,
  redirect,
} from "@tanstack/react-router";
import { AppLayout } from "@/app/layout";

// The app shell (AppLayout) is loaded eagerly so the window paints immediately.
// Every page below is code-split into its own chunk and fetched on demand —
// heavy pages (PDF preview, markdown chat) no longer bloat the initial bundle.
// With `defaultPreload: "intent"`, hovering a sidebar link preloads its chunk.
const rootRoute = createRootRoute({
  component: AppLayout,
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/inbox" });
  },
});

const inboxRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/inbox",
  component: lazyRouteComponent(() => import("@/app/inbox/page"), "InboxPage"),
});

const libraryRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/library",
  component: lazyRouteComponent(() => import("@/app/library/page"), "LibraryPage"),
});

const reviewRenamesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/library/review-renames",
  component: lazyRouteComponent(() => import("@/app/library/review-renames"), "ReviewRenamesPage"),
});

const searchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/search",
  component: lazyRouteComponent(() => import("@/app/search/page"), "SearchPage"),
});

const chatRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/chat",
  component: lazyRouteComponent(() => import("@/app/chat/page"), "ChatPage"),
});

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: lazyRouteComponent(() => import("@/app/settings/page"), "SettingsPage"),
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  inboxRoute,
  libraryRoute,
  reviewRenamesRoute,
  searchRoute,
  chatRoute,
  settingsRoute,
]);

export const router = createRouter({
  routeTree,
  defaultPreload: "intent",
  defaultPendingComponent: RoutePending,
});

function RoutePending() {
  return (
    <div className="grid min-h-[40vh] place-items-center" role="status" aria-live="polite">
      <span className="size-5 animate-spin rounded-full border-2 border-muted-foreground/30 border-t-foreground" />
      <span className="sr-only">Loading…</span>
    </div>
  );
}

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
