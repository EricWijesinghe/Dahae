// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ui/ui.dart';

const _custom = BrandConfig(
  name: 'Test Co',
  primary: Color(0xFF0B5D2A),
  primaryDeep: Color(0xFF07401C),
  fresh: Color(0xFF2E8B57),
  sun: Color(0xFFFFD95A),
);

void main() {
  test('contrast ratio matches WCAG for black on white', () {
    expect(contrastRatio(Colors.white, Colors.black), closeTo(21, 0.01));
  });

  test('white label text is allowed on dark brand colours only', () {
    expect(whiteTextReadsOn(BrandConfig.placeholder.primary), isTrue);
    expect(whiteTextReadsOn(_custom.primary), isTrue);
    expect(whiteTextReadsOn(const Color(0xFFFFD95A)), isFalse);
  });

  testWidgets('theme colours and the logo wordmark follow the BrandScope', (
    tester,
  ) async {
    late ColorScheme scheme;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(_custom, Brightness.light),
        home: BrandScope(
          brand: _custom,
          child: Builder(
            builder: (context) {
              scheme = Theme.of(context).colorScheme;
              return const DahaeLogo(wordmark: true);
            },
          ),
        ),
      ),
    );

    expect(scheme.primary, _custom.primary);
    expect(find.text('Test Co'), findsOneWidget);
  });

  testWidgets('the wordmark uses the English name in English', (tester) async {
    const brand = BrandConfig(
      name: '테스트',
      nameEn: 'Test Co',
      primary: Color(0xFF0B5D2A),
      primaryDeep: Color(0xFF07401C),
      fresh: Color(0xFF2E8B57),
      sun: Color(0xFFFFD95A),
    );
    for (final (locale, name) in [
      (const Locale('en'), 'Test Co'),
      (const Locale('ko'), '테스트'),
    ]) {
      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(brand, Brightness.light),
          home: Builder(
            builder: (context) => Localizations.override(
              context: context,
              locale: locale,
              child: const DahaeLogo(wordmark: true, brand: brand),
            ),
          ),
        ),
      );
      expect(find.text(name), findsOneWidget, reason: '$locale');
    }
  });

  testWidgets('lockupWidth matches the laid-out wordmark lockup', (
    tester,
  ) async {
    for (final (name, size) in [
      ('우집사', 34.0),
      ('Uzipsa', 34.0),
      ('Test Co', 48.0),
    ]) {
      final brand = BrandConfig(
        name: name,
        primary: _custom.primary,
        primaryDeep: _custom.primaryDeep,
        fresh: _custom.fresh,
        sun: _custom.sun,
      );
      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(brand, Brightness.light),
          home: Center(
            child: DahaeLogo(wordmark: true, size: size, brand: brand),
          ),
        ),
      );
      expect(find.text(name), findsOneWidget);
      expect(
        tester.getSize(find.byType(DahaeLogo)).width,
        closeTo(DahaeLogo.lockupWidth(name, size), 0.5),
        reason: '$name @ $size',
      );
    }
  });

  testWidgets(
    'an uploaded logo stands in for the flat, mono, 3D and wordmark art',
    (tester) async {
      const url =
          'https://shop.test/v1/files/0123456789abcdef0123456789abcdef.png';
      const brand = BrandConfig(
        name: 'Test Co',
        primary: Color(0xFF0B5D2A),
        primaryDeep: Color(0xFF07401C),
        fresh: Color(0xFF2E8B57),
        sun: Color(0xFFFFD95A),
        logoUrl: url,
      );
      bool showsUpload() => tester
          .widgetList<Image>(find.byType(Image))
          .any(
            (i) =>
                i.image is NetworkImage && (i.image as NetworkImage).url == url,
          );
      for (final v in DahaeLogoVariant.values) {
        await tester.pumpWidget(
          MaterialApp(
            theme: buildTheme(brand, Brightness.light),
            home: DahaeLogo(variant: v, brand: brand, animate: true),
          ),
        );
        expect(showsUpload(), isTrue, reason: v.name);
      }
      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(brand, Brightness.light),
          home: const BrandScope(brand: brand, child: Icon3D('brand_wordmark')),
        ),
      );
      expect(showsUpload(), isTrue, reason: 'brand_wordmark');
      // Network images fail in tests; the errors fall back to the built-in art.
      await tester.pumpAndSettle();
    },
  );

  testWidgets('outside a scope the built-in brand is used', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildTheme(BrandConfig.placeholder, Brightness.light),
        home: const DahaeLogo(wordmark: true),
      ),
    );

    expect(find.text(BrandConfig.placeholder.name), findsOneWidget);
  });
}
