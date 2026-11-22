// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:typed_data';

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';
import '../widgets/inline_button.dart';

/// Shows the made images (browser tab, app icon, home-screen icon, share card) before anything is uploaded.
/// Resolves true when the owner goes ahead.
Future<bool> confirmBrandPreview(
  BuildContext context,
  Map<String, Uint8List> files,
) async {
  final t = context.t;
  final ok = await showDialog<bool>(
    context: context,
    builder: (ctx) => AlertDialog(
      title: Text(t.cRbPreviewTitle),
      content: SingleChildScrollView(child: BrandPreviewGrid(files: files)),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(ctx).pop(false),
          child: Text(t.cancel),
        ),
        FilledButton(
          style: compactFilled,
          onPressed: () => Navigator.of(ctx).pop(true),
          child: Text(t.cSetupSave),
        ),
      ],
    ),
  );
  return ok ?? false;
}

/// The made images at a glance, each with what it is for.
class BrandPreviewGrid extends StatelessWidget {
  const BrandPreviewGrid({super.key, required this.files});
  final Map<String, Uint8List> files;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    Widget tile(String slot, String label, double w, double h) => Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        ClipRRect(
          borderRadius: BorderRadius.circular(slot == 'og_1200' ? 8 : w / 5),
          child: Image.memory(
            files[slot]!,
            width: w,
            height: h,
            fit: BoxFit.contain,
            filterQuality: FilterQuality.medium,
          ),
        ),
        const SizedBox(height: Space.xs),
        Text(label, style: context.text.labelSmall),
      ],
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(t.cRbPreviewBody, style: context.text.bodySmall),
        const SizedBox(height: Space.md),
        Wrap(
          spacing: Space.lg,
          runSpacing: Space.md,
          crossAxisAlignment: WrapCrossAlignment.end,
          children: [
            tile('favicon_32', t.cRbSlotFavicon, 32, 32),
            tile('icon_512', t.cRbSlotAppIcon, 72, 72),
            tile('maskable_512', t.cRbSlotMaskable, 72, 72),
          ],
        ),
        const SizedBox(height: Space.md),
        tile('og_1200', t.cRbSlotShare, 280, 147),
      ],
    );
  }
}

/// After Save: where the new brand shows, and when.
Future<void> showBrandApplied(BuildContext context) => showDialog<void>(
  context: context,
  builder: (ctx) => AlertDialog(
    title: Text(ctx.t.cRbAppliedTitle),
    content: Text(ctx.t.cRbAppliedList),
    actions: [
      FilledButton(
        style: compactFilled,
        onPressed: () => Navigator.of(ctx).pop(),
        child: Text(ctx.t.ok),
      ),
    ],
  ),
);
