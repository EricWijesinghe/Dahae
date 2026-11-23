// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

/// Pyeong range each layout usually fits (C-03).
/// shortcut: rough home norms, not yet confirmed by the owner; move them to the catalogue when the owner sets them.
const _layoutPyeong = <String, (int, int)>{
  'studio': (1, 10),
  'one_and_half': (1, 10),
  'two_rooms': (10, 999),
  'three_rooms': (20, 999),
  'four_plus': (30, 999),
};

/// True when [pyeong] lies outside the range the [layout] usually fits.
bool layoutSizeMismatch(String? layout, int pyeong) {
  final range = _layoutPyeong[layout];
  return range != null && (pyeong < range.$1 || pyeong > range.$2);
}

/// Inline note under the address when the chosen layout and the saved size disagree (C-03).
class QuoteSizeNote extends StatelessWidget {
  const QuoteSizeNote({
    super.key,
    required this.layout,
    required this.sizePyeong,
    required this.onChangeLayout,
    required this.onChangeSize,
  });
  final String layout;
  final int sizePyeong;
  final VoidCallback onChangeLayout;
  final VoidCallback onChangeSize;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: Space.gutter,
        vertical: Space.sm,
      ),
      child: Container(
        padding: const EdgeInsets.all(Space.md),
        decoration: BoxDecoration(
          color: context.colors.surface,
          borderRadius: BorderRadius.circular(Radii.card),
          border: Border.all(color: context.colors.danger),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            KeepAllText(
              t.quoteSizeTitle,
              style: context.text.titleSmall?.copyWith(
                fontWeight: FontWeight.w800,
              ),
            ),
            const SizedBox(height: Space.xs),
            KeepAllText(
              t.quoteSizeBody(t.pyeong('$sizePyeong'), layoutLabel(t, layout)),
              style: context.text.bodySmall?.copyWith(
                color: context.colors.inkMuted,
              ),
            ),
            Wrap(
              spacing: Space.sm,
              children: [
                TextButton(
                  onPressed: onChangeLayout,
                  child: KeepAllText(t.quoteSizeChangeLayout),
                ),
                TextButton(
                  onPressed: onChangeSize,
                  child: KeepAllText(t.quoteSizeChangeSize),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
