My added notes:
You've hardcoded some color choices in, don't do this, have them depend on the theme.
The system we have that makes the active tab depend on the document theme is so the document 'paper' background is continuous with the tab flowing into it. But since the flow is a themed element, we should just make the focused tab themed when any tab except a .db8 tab is focused.
Only cells in the first column of a flow type spawn correctly. Cells in the next column over spawn in horizontal alignment with the first column over, but raised on the screen. They start cramming and just incrementing on the first column.
You've sort of given the flow tool an internal ribbon, but clearly recognized the actual ribbon is modal by not showing the document ribbon when a .fl0 is open. Can we axe the internal ribbon and just have those functionalities get taken up modally by the flow ribbon? Have the UI here follow the same style as the UI in the document ribbon please. That means use the same component types, and try to find icons from the icon libraries we have installed this time.
You've used an internal flow sheet sorter to the right of the outline, but the outline is still an element that doesn't have use when a .fl0 is open. Can we have the outline be modal, and display the flow orders using the outline UI already built with the gpui-component? And yes, this must be sortable now:
  - Sortable drag-reorder tab behavior: debate-flow/src/routes/app/+page.svelte:115,
    debate-flow/src/lib/components/SortableList.svelte:2.
XLSX export/settings import----only do this if you're sure you can deterministically match the logic that the flower app uses with extremely fast Rust code and the fastest Rust libraries available for the option, while being architecturally sound: debate-flow/src/lib/models/file.ts:44,
    debate-flow/src/lib/models/file.ts:49, debate-flow/src/lib/models/file.ts:118.

  - Collaborative editing of the flow is getting deferred for later along with collaborative editing of the document. We're going to use p2p connections with Iroh later and a CRDT solution. Just make sure the flow is entirely architecturally correct for us to do this with later.: debate-flow/src/routes/app/
    +page.svelte:345, debate-flow/src/lib/models/sharingChannel.ts:8, debate-flow/
    src/lib/models/sharingConnection.ts:6, debate-flow/src/lib/models/
    nodeAction.ts:13.
  - Defer timer for now.: debate-flow/src/lib/components/Timers.svelte:17, debate-flow/
    src/lib/models/debateStyle.ts:72.
    src/lib/components/SideDoc.svelte:7.
