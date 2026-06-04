export type LyricLine = {
  time: number;
  text: string;
};

export type ParsedLrc = {
  metadata: Record<string, string>;
  lines: LyricLine[];
};

const TIMESTAMP_PATTERN = /\[(\d{1,3}):(\d{2})(?:[.:](\d{1,3}))?\]/g;
const METADATA_PATTERN = /^\[([a-zA-Z]+):(.+)\]$/;

export function parseLrc(source: string): ParsedLrc {
  const metadata: Record<string, string> = {};
  const lines: LyricLine[] = [];

  for (const rawLine of source.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line) {
      continue;
    }

    const timestamps = [...line.matchAll(TIMESTAMP_PATTERN)];
    if (timestamps.length === 0) {
      const metadataMatch = line.match(METADATA_PATTERN);
      if (metadataMatch) {
        metadata[metadataMatch[1].toLowerCase()] = metadataMatch[2].trim();
      }
      continue;
    }

    const text = line.replace(TIMESTAMP_PATTERN, "").trim();
    for (const timestamp of timestamps) {
      lines.push({
        time: parseTimestamp(timestamp),
        text
      });
    }
  }

  lines.sort((a, b) => a.time - b.time);

  return {
    metadata,
    lines
  };
}

export function findActiveLyricLine(lines: LyricLine[], time: number): number {
  if (lines.length === 0 || time < lines[0].time) {
    return -1;
  }

  let low = 0;
  let high = lines.length - 1;

  while (low <= high) {
    const mid = Math.floor((low + high) / 2);
    const current = lines[mid].time;
    const next = lines[mid + 1]?.time ?? Number.POSITIVE_INFINITY;

    if (time >= current && time < next) {
      return mid;
    }

    if (time < current) {
      high = mid - 1;
    } else {
      low = mid + 1;
    }
  }

  return -1;
}

function parseTimestamp(match: RegExpMatchArray): number {
  const minutes = Number(match[1]);
  const seconds = Number(match[2]);
  const fraction = match[3] ?? "0";
  const fractionSeconds = Number(fraction.padEnd(3, "0").slice(0, 3)) / 1000;

  return minutes * 60 + seconds + fractionSeconds;
}
