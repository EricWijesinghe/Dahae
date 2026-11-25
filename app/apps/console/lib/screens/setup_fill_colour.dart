// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import 'setup_business.dart' show parseHex;

/// The apps' light-mode card surface, where the fill colour is used.
const _appSurface = Colors.white;

/// Business profile `fill_color`: the colour of buttons and selected items in the apps' light mode. Empty means
/// the brand (primary) colour. Any colour is allowed: the apps put white or dark text on it, whichever reads,
/// and the preview shows that label with its contrast.
class FillColourField extends StatelessWidget {
  const FillColourField({
    super.key,
    required this.controller,
    required this.primary,
  });

  final TextEditingController controller;

  /// The brand colour being edited above: the fill when this field is empty, and the link colour.
  final Color primary;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final fill = parseHex(controller.text) ?? primary;
    final label = labelOnFill(fill);
    final ratio = contrastRatio(label, fill).toStringAsFixed(1);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        KeepAllText(t.cSetupFillColor, style: context.text.labelLarge),
        const SizedBox(height: Space.xxs),
        KeepAllText(t.cSetupFillColorHint, style: context.text.bodySmall),
        const SizedBox(height: Space.xs),
        Wrap(
          spacing: Space.sm,
          runSpacing: Space.xs,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            SizedBox(
              width: 130,
              child: TextField(
                key: const ValueKey('fill_color'),
                controller: controller,
                decoration: InputDecoration(
                  labelText: '${t.cSetupFillColor} #RRGGBB',
                  isDense: true,
                ),
              ),
            ),
            // A picture of the apps' light mode (white surface), not a control of this screen.
            ExcludeSemantics(
              child: Container(
                key: const ValueKey('fill_preview'),
                padding: const EdgeInsets.all(Space.xs),
                decoration: BoxDecoration(
                  color: _appSurface,
                  border: Border.all(color: context.colors.line),
                  borderRadius: BorderRadius.circular(Radii.input),
                ),
                child: Wrap(
                  spacing: Space.md,
                  runSpacing: Space.xs,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: [
                    Container(
                      constraints: const BoxConstraints(minHeight: 48),
                      padding: const EdgeInsets.symmetric(horizontal: Space.lg),
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        color: fill,
                        borderRadius: BorderRadius.circular(Radii.input),
                      ),
                      child: KeepAllText(
                        t.cSetupFillSample,
                        style: context.text.labelLarge?.copyWith(color: label),
                      ),
                    ),
                    KeepAllText(
                      t.cSetupFillLinkSample,
                      style: context.text.labelMedium?.copyWith(
                        color: primary,
                        decoration: TextDecoration.underline,
                        decorationColor: primary,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: Space.xs),
        KeepAllText(
          label == Colors.white
              ? t.cSetupFillLabelWhite(ratio)
              : t.cSetupFillLabelDark(ratio),
          style: context.text.bodySmall,
        ),
      ],
    );
  }
}
