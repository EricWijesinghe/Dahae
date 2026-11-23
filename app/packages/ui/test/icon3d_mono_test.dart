// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ui/ui.dart';

// Built once: a fresh ThemeData per pump would start an AnimatedTheme lerp (a ticker).
final _themes = {
  for (final b in Brightness.values) b: buildTheme(BrandConfig.placeholder, b),
};

// No theme lerp: a light-to-dark switch paints the new theme on the first frame.
Widget _app(Widget child, {Brightness b = Brightness.light}) => MaterialApp(
  theme: _themes[b],
  themeAnimationDuration: Duration.zero,
  home: Scaffold(body: Center(child: child)),
);

/// The tint and blend mode the art [Image] of [id] is drawn with.
(Color?, BlendMode?) _paint(WidgetTester tester, String id) {
  final img = tester.widget<Image>(
    find.descendant(
      of: find.byKey(ValueKey('icon3d:$id')),
      matching: find.byType(Image),
    ),
  );
  return (img.color, img.colorBlendMode);
}

/// True when every visible pixel of the file is white: a line glyph mask,
/// invisible on the light canvas unless tinted.
Future<bool> _isWhiteMask(String path) async {
  final codec = await ui.instantiateImageCodec(File(path).readAsBytesSync());
  final image = (await codec.getNextFrame()).image;
  final px = (await image.toByteData(
    format: ui.ImageByteFormat.rawStraightRgba,
  ))!.buffer.asUint8List();
  image.dispose();
  codec.dispose();
  var seen = 0;
  for (var i = 0; i < px.length; i += 4) {
    if (px[i + 3] <= 32) continue;
    seen++;
    if (px[i] <= 230 || px[i + 1] <= 230 || px[i + 2] <= 230) return false;
  }
  return seen > 0;
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('mono is set exactly on the ids whose art is a white glyph', (
    tester,
  ) async {
    final wrong = <String>[];
    var mono = 0;
    // Real image decoding needs the real event loop, not the fake-async clock.
    await tester.runAsync(() async {
      for (final e in Icon3DRegistry.specs.entries) {
        // assetFor resolves an alias: its target file is judged against the
        // alias entry's own flag, which is what Icon3D paints with.
        final files = {
          for (final d in ['', '2.0x/', '3.0x/'])
            for (final idle in [false, if (e.value.hasIdle) true])
              Icon3DRegistry.assetFor(
                e.key,
                idle: idle,
              ).replaceFirst('icons3d/', 'icons3d/$d'),
        }.where((f) => File(f).existsSync());
        for (final f in files) {
          if (await _isWhiteMask(f) != e.value.mono) wrong.add('${e.key} ($f)');
        }
        if (e.value.mono) mono++;
      }
    });
    expect(wrong, isEmpty, reason: 'white glyph art must be mono: true');
    // The cut-from-PDF set (nav, bell, menu, aircon, tools, move flow...),
    // bld_villa (an alias of the bld_house glyph) included.
    expect(mono, 62);
  });

  testWidgets('mono art is tinted ink, colour art is drawn as shipped', (
    tester,
  ) async {
    final c = appColorsFor(BrandConfig.placeholder, false);
    await tester.pumpWidget(
      _app(
        const Column(
          children: [
            Icon3D('ui_bell'),
            Icon3D('menu_faq'),
            Icon3D('ac_wall', active: true),
            Icon3D('svc_studio'),
          ],
        ),
      ),
    );
    expect(_paint(tester, 'ui_bell'), (c.ink, BlendMode.srcIn));
    expect(_paint(tester, 'menu_faq'), (c.ink, BlendMode.srcIn));
    // A selected card still reads ink: only tab icons with idle art dim.
    expect(_paint(tester, 'ac_wall'), (c.ink, BlendMode.srcIn));
    expect(_paint(tester, 'svc_studio'), (null, null));
  });

  testWidgets('tab icons: active ink, idle muted ink, in both themes', (
    tester,
  ) async {
    for (final b in Brightness.values) {
      final c = appColorsFor(BrandConfig.placeholder, b == Brightness.dark);
      await tester.pumpWidget(
        _app(
          const Row(
            children: [Icon3D('nav_home', active: true), Icon3D('nav_chat')],
          ),
          b: b,
        ),
      );
      expect(_paint(tester, 'nav_home'), (
        c.ink,
        BlendMode.srcIn,
      ), reason: '$b');
      expect(_paint(tester, 'nav_chat'), (
        c.inkMuted,
        BlendMode.srcIn,
      ), reason: '$b');
      // Both tints clear the 3:1 non-text contrast bar on the bar's surface.
      expect(contrastRatio(c.ink, c.surface), greaterThanOrEqualTo(3));
      expect(contrastRatio(c.inkMuted, c.surface), greaterThanOrEqualTo(3));
    }
  });

  testWidgets('per-id tones: muted placeholder, brand wordmark', (
    tester,
  ) async {
    final c = appColorsFor(BrandConfig.placeholder, false);
    await tester.pumpWidget(
      _app(
        const Row(
          children: [
            Icon3D('ui_image_placeholder'),
            Icon3D('brand_wordmark'),
            Icon3D('bld_villa', active: true),
          ],
        ),
      ),
    );
    expect(_paint(tester, 'ui_image_placeholder'), (
      c.inkMuted,
      BlendMode.srcIn,
    ));
    final primary = _themes[Brightness.light]!.colorScheme.primary;
    expect(_paint(tester, 'brand_wordmark'), (primary, BlendMode.srcIn));
    // 빌라/주택 draws the house glyph, tinted like the rest of its row.
    expect(_paint(tester, 'bld_villa'), (c.ink, BlendMode.srcIn));
    expect(
      Icon3DRegistry.assetFor('bld_villa'),
      'assets/icons3d/bld_house.webp',
    );
  });

  testWidgets('partner chat/more tabs keep their colour clay art', (
    tester,
  ) async {
    await tester.pumpWidget(
      _app(
        const Row(
          children: [Icon3D('p_nav_chat', active: true), Icon3D('p_nav_more')],
        ),
      ),
    );
    expect(_paint(tester, 'p_nav_chat'), (null, null));
    expect(_paint(tester, 'p_nav_more'), (null, null));
  });

  testWidgets('an explicit colour paints mono art only', (tester) async {
    await tester.pumpWidget(
      _app(
        const Row(
          children: [
            Icon3D('elev_yes', active: true, color: Colors.white),
            Icon3D('svc_move', color: Colors.white),
          ],
        ),
      ),
    );
    expect(_paint(tester, 'elev_yes'), (Colors.white, BlendMode.srcIn));
    expect(_paint(tester, 'svc_move'), (null, null));
  });
}
