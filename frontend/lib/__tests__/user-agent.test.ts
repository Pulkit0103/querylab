import { describe, expect, it } from "vitest";

import { describeUserAgent } from "@/lib/user-agent";

describe("describeUserAgent", () => {
  it("names the browser and the system", () => {
    expect(
      describeUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36 Edg/130.0"),
    ).toBe("Edge on Windows");
    expect(describeUserAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15")).toBe(
      "Safari on macOS",
    );
    expect(describeUserAgent("Mozilla/5.0 (X11; Linux x86_64; rv:131.0) Gecko/20100101 Firefox/131.0")).toBe("Firefox on Linux");
    expect(describeUserAgent("curl/8.4.0")).toBe("curl");
    expect(describeUserAgent(null)).toBe("Unknown device");
  });
});
