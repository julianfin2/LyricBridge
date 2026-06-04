const YOUTUBE_HOSTS = new Set([
  "www.youtube.com",
  "youtube.com",
  "m.youtube.com",
  "music.youtube.com",
  "youtu.be"
]);

export function parseYouTubeVideoId(rawUrl: string): string | null {
  let url: URL;

  try {
    url = new URL(rawUrl);
  } catch {
    return null;
  }

  if (!YOUTUBE_HOSTS.has(url.hostname)) {
    return null;
  }

  if (url.hostname === "youtu.be") {
    return normalizeVideoId(url.pathname.slice(1));
  }

  const watchId = url.searchParams.get("v");
  if (watchId) {
    return normalizeVideoId(watchId);
  }

  const shortsMatch = url.pathname.match(/^\/shorts\/([^/?#]+)/);
  if (shortsMatch) {
    return normalizeVideoId(shortsMatch[1]);
  }

  const embedMatch = url.pathname.match(/^\/embed\/([^/?#]+)/);
  if (embedMatch) {
    return normalizeVideoId(embedMatch[1]);
  }

  return null;
}

function normalizeVideoId(value: string): string | null {
  const id = value.trim();
  return /^[a-zA-Z0-9_-]{11}$/.test(id) ? id : null;
}
