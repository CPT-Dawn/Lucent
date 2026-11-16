# Usage

What a Popup shows and how it behaves. Anything that sends desktop notifications works with Lucent, `notify-send` included.

## Try it

These are the Popups in the README's screenshots:

```sh
notify-send "Build finished" "All tests passed"
notify-send -u low -a Syncthing -i folder-sync "Folder synced" "Documents is up to date"
notify-send -a Volume -i multimedia-volume-control -h int:value:65 "Volume" "65%"
notify-send -a Calendar -i x-office-calendar -A open=Open -A snooze=Snooze \
  "Design review in 5 minutes" "Room 3, with <b>Alex</b> and <i>Priya</i>"
notify-send -u critical -a Power -i battery-caution "Battery low" "5% remaining. Plug in now."
```

`-A` makes `notify-send` wait and print the Action you click.

## What a Popup shows

| Feature | Behaviour |
| --- | --- |
| Urgency | Critical Popups take the `critical` colour and never time out. Low ones are faded so they don't compete for attention. |
| Actions | Shown as buttons. |
| Progress | A bar, from the `value` hint. |
| Icons | From your icon theme (PNG or SVG), or an image the app sends (album art, avatars). |
| Markup | `<b>`, `<i>`, `<u>` and `<a>` links in the body. |

## The mouse

| Input | Default |
| --- | --- |
| Left-click | Run the Default action and dismiss the Popup, bringing the app's window to the front if it supports activation |
| Middle-click | Dismiss the Popup |
| Right-click | Dismiss every Popup, and any waiting in the Queue |
| Hover | Pause the timeout until the pointer leaves |

Popups never take keyboard focus. Change the buttons with `left_click`, `middle_click` and `right_click` in [Configuration](configuration.md#layout-and-behaviour).

## Stack and Queue

Up to five Popups are shown at once (`max_visible`), newest nearest the anchor corner. The rest wait in the Queue, shown as "+N more", and appear as space frees up.

A Notification that replaces another (a volume change, say) updates its Popup in place, even if it reuses an id whose Popup has gone, as `notify-send -r 9999` scripts do. So does one with the same synchronous Tag (`x-canonical-private-synchronous` or `x-dunst-stack-tag`), as dunst and mako scripts send. With a `value` hint as well, it shows as the [OSD](osd.md).

[Back to the README](../README.md)
