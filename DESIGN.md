---
name: StoreForge
description: A compact desktop workspace for browsing and operating on object storage.
colors:
  accent: "#037c96"
  accent-deep: "#006780"
  signal: "#72e3ff"
  selected: "#e5f6fb"
  success: "#137758"
  danger: "#a2283a"
  danger-bg: "#fbeef0"
  navy: "#071e34"
  rail: "#0a2238"
  rail-ink: "#edf7ff"
  rail-muted: "#a9bdce"
  rail-rule: "#284056"
  page: "#f5f8fa"
  surface: "#ffffff"
  surface-alt: "#f1f5f8"
  ink: "#102437"
  muted: "#506778"
  line: "#dce5eb"
  line-strong: "#c5d3de"
typography:
  display:
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif'
    fontSize: "clamp(30px, 3.2vw, 42px)"
    fontWeight: 710
    lineHeight: 1.1
    letterSpacing: "-.038em"
  headline:
    fontSize: "21px"
    fontWeight: 680
    lineHeight: 1.2
    letterSpacing: "-.022em"
  title:
    fontSize: "14px"
    fontWeight: 680
  body:
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif'
    fontSize: "14px"
    lineHeight: 1.45
  label:
    fontSize: "12.5px"
    fontWeight: 640
  mono:
    fontFamily: '"SFMono-Regular", Consolas, "Liberation Mono", monospace'
rounded:
  chip: "4px"
  control: "7px"
  panel: "9px"
  dialog: "12px"
spacing:
  control-gap: "7px"
  inline-gap: "8px"
  panel-inset: "16px"
  listing-inset: "25px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.surface}"
    rounded: "{rounded.control}"
    padding: "6px 11px"
  button-primary-hover:
    backgroundColor: "{colors.accent-deep}"
  button-secondary:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
    padding: "6px 11px"
  button-subtle:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
  button-danger:
    backgroundColor: "{colors.danger}"
    textColor: "{colors.surface}"
    rounded: "{rounded.control}"
  button-danger-outline:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.danger}"
    rounded: "{rounded.control}"
  input-text:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
    padding: "7px 10px"
  app-nav:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.muted}"
    height: "59px"
  chip:
    backgroundColor: "{colors.surface-alt}"
    textColor: "{colors.muted}"
    rounded: "{rounded.chip}"
    padding: "1px 6px"
  settings-panel:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.panel}"
  object-table:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.control}"
  object-row-focused:
    backgroundColor: "{colors.selected}"
    textColor: "{colors.ink}"
---

# Design System: StoreForge

## Overview

**Creative North Star: "Accession Register"**

The workspace pairs a deep navy connection rail with cool white work surfaces, using the European Space Agency palette as a reference. The supplied bucket-and-anvil mark anchors the brand.

Dense object lists and property rows are divided by fine rules. The active connection, current path, selected object, and operation results have distinct places so operators can verify what they are working on.

**Key Characteristics:**

- Navy connection rail and cyan interaction states
- Task-focused tables and property lists
- Flat work surfaces and restrained overlays

## Colors

### Primary

- **Deep Cyan** fills primary actions. **Darker Cyan** supplies the hover fill and the text color for active workspace destinations.
- **Signal Cyan** highlights the active bucket and outlines keyboard focus on the navy rail.
- **Pale Cyan** ties focused object rows, selected settings, and running jobs to their active state.

### Secondary

- **Green** indicates success or an accepted operation. **Crimson** signals destructive actions and failures. Pair each status color with a label or icon.

### Neutral

- **Rail Navy** holds the connection selector and bucket navigation, using pale text for item names and muted blue-gray for supporting details.
- **Cool White** frames the workspace. **White** separates tables, panels, and forms from the surrounding canvas.
- **Ink** carries names and headings. **Slate** carries labels and secondary values. Fine blue-gray rules separate rows and bound controls.

## Typography

The system sans stack supports native desktop rendering. Use the monospace stack for literal keys, identifiers, code, and raw responses. Tabular numerals keep sizes, dates, and counts steady.

- **Display** (30–42px, 710) names the current workspace or location.
- **Headline** (21px, 680) names panels and report sections.
- **Title** (14px, 680) introduces compact groups of properties or controls.
- **Body** (14px, 1.45 line height) supports general copy. The object table uses 11.5px data and 12px object names.
- **Label** (12.5px, 640) identifies form fields. Table headings and inspector labels use 11px and 10.5px respectively.

## Layout

The desktop shell pairs a 230px connection rail with a flexible work area. A 59px workspace bar and a 29px status bar frame the independently scrolling content. Opening an object adds a 312px inspector alongside the listing at wide widths.

The rail and inspector narrow at 1,210px and 1,020px. At 865px and below, an open inspector takes the full work area and hides the listing while preserving the rail. At 660px and below, the shell uses one column and the rail overlays the workspace when open.

The object register uses 39px rows by default and 31px rows in Compact mode. Preserve space for object names first. Keep row separators and numeric columns aligned across adjacent rows.

## Elevation & Depth

Navigation, tables, settings panels, and inspector surfaces use tonal separation and fine rules. Floating menus use a soft shadow with a downward offset. Modal dialogs add a darker cast shadow and a translucent backdrop to mark the interruption.

## Shapes

Chips have 4px corners. Controls and table frames use 7px, settings panels 9px, and modals 12px. Thin borders keep dense rows and adjacent controls separated at small sizes.

## Components

### Buttons and fields

Use a cyan fill for the primary action and a white bordered button for adjacent actions. Subtle buttons carry compact controls such as closing a pane or revealing details. Use outlined crimson for secondary destructive entry points and a filled crimson button for destructive confirmation.

Buttons start at 34px high. Text fields start at 36px and use a strong border with a white fill. Keyboard focus uses a 2px outline offset by 2px. Disabled controls reduce opacity. A running refresh may rotate its icon and suppresses rotation when reduced motion is requested.

### Navigation and chips

Active workspace tabs use cyan text and a 2px underline. The active bucket uses a lighter navy fill and a 1px cyan edge. Neutral chips pair a pale surface with slate text. Running, successful, and failed states keep text labels with their color.

### Object register and inspector

Use a fixed-layout table with aligned data columns. Folder and file glyphs sit in small tinted squares. Opening an item marks its row with pale cyan and a 1px edge. Checkboxes remain separate from the action that opens the record.

The inspector has a dedicated header, section tabs, and a scrollable body. Group technical fields with thin rules and allow long keys and identifiers to wrap. Closing the narrow inspector returns the operator to the listing.

### Settings and operation results

Settings panels group related rows within a white bordered surface. Activity entries show status and processed counts at a glance. Expanded entries pair a thin progress meter with separate succeeded and failed counts and details for failed items.

## Do's and Don'ts

- **Do** keep the selected endpoint and exact storage context visible near actions.
- **Do** use fine dividers and cool surface changes to group data.
- **Do** pair status colors with labels and preserve clear keyboard focus.
- **Don't** use shadows to separate resting table rows or settings sections.
- **Don't** use the saturated rail accent as body text on light surfaces.
