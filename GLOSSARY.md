# Lucent

A notification daemon for Wayland compositors that put lightness first. It receives notifications from applications and shows them briefly on screen.

## Language

**Notification**:
A message an application asks Lucent to show, received via the freedesktop notifications protocol.
_Avoid_: Alert, message, toast

**Popup**:
The on-screen rendering of a single Notification.
_Avoid_: Window, card, bubble, toast

**OSD**:
The single compact Popup that shows a level such as volume or brightness, placed apart from the Stack. A Notification is shown as the OSD when it carries a synchronous tag and a `value` hint.
_Avoid_: Overlay, indicator, HUD

**Tag**:
A sender-chosen name (the `x-canonical-private-synchronous` or `x-dunst-stack-tag` hint) under which a Notification replaces the live one with the same Tag.
_Avoid_: Stack tag, sync id

**Stack**:
The Popups currently visible, ordered from the anchor edge outward.
_Avoid_: List, column

**Queue**:
Notifications accepted but waiting for room in the Stack.
_Avoid_: Pending, backlog

**Urgency**:
The sender-declared importance of a Notification: low, normal or critical.
_Avoid_: Priority, severity

**Action**:
A labelled choice offered by the sender; invoking one is reported back to the sender.
_Avoid_: Button, command

**Theme**:
The complete set of visual settings applied to Popups.
_Avoid_: Skin, style, look

**Preset**:
A named, built-in Theme the user can select and then override; the default Preset is Midnight Bloom.
_Avoid_: Skin, template

**Default action**:
The Action a sender marks as the one to run when the Popup itself is clicked.
_Avoid_: Primary action, click action

**Rule**:
A Theme override that applies only to Notifications matching certain criteria (app name, Urgency).
_Avoid_: Filter, profile

**Idle exit**:
Lucent ending its own process once the Stack and Queue are empty, to be restarted on demand by the next Notification.
_Avoid_: Shutdown, sleep
