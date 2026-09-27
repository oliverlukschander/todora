# Release pages

Visitors to https://todora.lukschander.com/ land on the latest GitHub release.
Write each page for someone who has not played Todora: a gameplay image,
a short introduction, direct platform download links, then the changes.
Keep platform requirements and signing limitations beside the downloads.

The launch kit is on `promo/launch-kit`, commit `58b7b7f`, in
`marketing/promo/`. Its README documents the footage, exports and credits.
The large video files are local exports, not git objects.

For v0.11.1 we published these release attachments:

- `todora-hero.jpg`: `exports/graphics/todora-web-hero-2560x1440.jpg`.
- `todora-30s-16x9.mp4`: `exports/video/todora-30s-16x9.mp4`.

The hero links to the trailer, with a text link underneath. This works without
depending on GitHub embedding a release-attachment video player. Both show
features in the v0.11.1 download; the kit deliberately avoids a circuit count.

For future releases, reuse these assets where still accurate or export new
footage. Upload media before linking it, use versioned release-asset URLs, and
keep the audio and elevation attribution at the bottom of the page. Publish
the checked-in notes with `gh release edit TAG --notes-file docs/releases/TAG.md`.
Confirm the public image, trailer and download links work, and that the site's
redirect reaches the intended release. Never replace an old release's game
downloads with a newer build.
