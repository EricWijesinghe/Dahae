// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ui/ui.dart';

/// An owner who picks a light sky fill and keeps a dark teal primary for text.
const _sky = BrandConfig(
  name: '우집사',
  primary: Color(0xFF00838F),
  primaryDeep: Color(0xFF006A74),
  fresh: Color(0xFF1E9EBA),
  sun: Color(0xFFFFD95A),
  fill: Color(0xFF00BCD4),
);

Color _textColour(WidgetTester tester, String label) => tester
    .widget<RichText>(
      find.descendant(of: find.text(label), matching: find.byType(RichText)),
    )
    .text
    .style!
    .color!;

void main() {
  group('labelOnFill', () {
    test(
      'white on dark fills, dark ink on light ones, black in the grey band',
      () {
        expect(labelOnFill(BrandConfig.placeholder.primary), Colors.white);
        expect(labelOnFill(const Color(0xFF00BCD4)), fillInk);
        // #777777: white 4.48:1 and #0B1217 4.2:1 both fall short; black reads.
        expect(labelOnFill(const Color(0xFF777777)), const Color(0xFF000000));
      },
    );

    test('every fill gets a label at 4.5:1 or better', () {
      for (var r = 0; r < 256; r += 17) {
        for (var g = 0; g < 256; g += 17) {
          for (var b = 0; b < 256; b += 17) {
            final fill = Color.fromARGB(255, r, g, b);
            expect(
              contrastRatio(labelOnFill(fill), fill),
              greaterThanOrEqualTo(minTextContrast),
              reason: '$fill',
            );
          }
        }
      }
    });
  });

  test('an unset fill is the primary colour', () {
    expect(BrandConfig.placeholder.fill, BrandConfig.placeholder.primary);
  });

  group('light theme with a light fill', () {
    final theme = buildTheme(_sky, Brightness.light);
    final c = theme.extension<AppColors>()!;

    test('filled controls are flat fill with the computed label', () {
      expect(c.brandStart, _sky.fill);
      expect(c.brandEnd, _sky.fill, reason: 'flat, no gradient');
      expect(c.onFill, fillInk);
      final style = theme.filledButtonTheme.style!;
      expect(style.backgroundColor!.resolve({}), _sky.fill);
      expect(style.foregroundColor!.resolve({}), fillInk);
      expect(theme.chipTheme.selectedColor, _sky.fill);
      expect(theme.chipTheme.checkmarkColor, fillInk);
      final label = theme.chipTheme.labelStyle!.color!;
      expect(
        WidgetStateProperty.resolveAs(label, {WidgetState.selected}),
        fillInk,
      );
      expect(WidgetStateProperty.resolveAs(label, <WidgetState>{}), c.ink);
    });

    test(
      'text and links take the primary, darkened to read on tinted surfaces',
      () {
        final p = theme.colorScheme.primary;
        expect(c.accent, p);
        expect(theme.colorScheme.onPrimary, Colors.white);
        // #00838F is 4.0:1 on the tint (the '1/6' step badge, the suggested chat reply): it must darken.
        expect(
          contrastRatio(_sky.primary, c.primaryTint),
          lessThan(minTextContrast),
        );
        expect(p, isNot(_sky.primary));
        for (final bg in [c.primaryTint, c.canvas, c.surfaceHigh, c.surface]) {
          expect(
            contrastRatio(p, bg),
            greaterThanOrEqualTo(minTextContrast),
            reason: '$bg',
          );
        }
        expect(
          contrastRatio(Colors.white, p),
          greaterThanOrEqualTo(minTextContrast),
        );
        // Darkened only as far as needed: still recognisably the owner's teal.
        expect(
          HSLColor.fromColor(p).hue,
          closeTo(HSLColor.fromColor(_sky.primary).hue, 1),
        );
      },
    );

    test('a primary that already reads is left alone', () {
      const green = BrandConfig(
        name: '청소왕',
        primary: Color(0xFF0B5D2A),
        primaryDeep: Color(0xFF084A21),
        fresh: Color(0xFF1E9EBA),
        sun: Color(0xFFFFD95A),
      );
      expect(
        buildTheme(green, Brightness.light).colorScheme.primary,
        green.primary,
      );
    });

    testWidgets('a rendered button label reads and a link stays primary', (
      tester,
    ) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: theme,
          home: Scaffold(
            body: Column(
              children: [
                FilledButton(onPressed: () {}, child: const Text('다음')),
                PrimaryButton(label: '예약 신청하기', onPressed: () {}),
                TextButton(onPressed: () {}, child: const Text('이용약관')),
              ],
            ),
          ),
        ),
      );
      for (final label in ['다음', '예약 신청하기']) {
        final fg = _textColour(tester, label);
        expect(fg, fillInk, reason: label);
        expect(contrastRatio(fg, _sky.fill), greaterThanOrEqualTo(4.5));
      }
      expect(_textColour(tester, '이용약관'), theme.colorScheme.primary);
    });
  });

  test('a dark fill keeps white labels', () {
    final c = appColorsFor(BrandConfig.placeholder, false);
    expect(c.onFill, Colors.white);
    expect(c.brandStart, BrandConfig.placeholder.primary);
  });

  test('dark mode ignores the fill colour', () {
    final theme = buildTheme(_sky, Brightness.dark);
    final c = theme.extension<AppColors>()!;
    expect(c.brandStart, _sky.primary);
    expect(c.brandEnd, _sky.primaryDeep);
    expect(c.onFill, Colors.white);
    expect(
      theme.filledButtonTheme.style!.backgroundColor!.resolve({}),
      _sky.primary,
    );
    expect(theme.chipTheme.selectedColor, c.primaryTint);
  });
}
