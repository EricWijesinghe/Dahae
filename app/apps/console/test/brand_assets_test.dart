// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:console/brand_assets.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// Alpha of the top-left pixel of a PNG.
Future<int> _cornerAlpha(Uint8List png) async {
  final image = await decodeLogo(png);
  final rgba = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
  return rgba!.getUint8(3);
}

void main() {
  const teal = Color(0xFF187A8F);

  test('the PNG header size reader reads width and height', () {
    final header = Uint8List(24);
    ByteData.sublistView(header)
      ..setUint32(16, 1200)
      ..setUint32(20, 630);
    expect(pngSize(header), (1200, 630));
    expect(pngSize(Uint8List(10)), isNull);
  });

  testWidgets('a letter mark is an opaque square PNG', (tester) async {
    await tester.runAsync(() async {
      final mark = await letterMark('청소왕', teal);
      expect(pngSize(mark), (512, 512));
      expect(await _cornerAlpha(mark), 255);
    });
  });

  testWidgets(
    'every slot comes out at its exact size, and the filled ones are opaque in the corners',
    (tester) async {
      await tester.runAsync(() async {
        final logo = await decodeLogo(await letterMark('A', teal));
        final assets = await deriveBrandAssets(
          logo: logo,
          color: teal,
          name: '청소왕',
          tagline: '집안일, 다 해드려요',
        );
        expect(assets.keys.toSet(), brandSlots.keys.toSet());
        for (final MapEntry(:key, :value) in assets.entries) {
          expect(pngSize(value), brandSlots[key], reason: key);
        }
        for (final slot in [
          'store_1024',
          'maskable_512',
          'apple_180',
          'og_1200',
        ]) {
          expect(await _cornerAlpha(assets[slot]!), 255, reason: slot);
        }
      });
    },
  );

  test('remaking is needed when a brand input changed or no images exist', () {
    const saved = {
      'name': '다해',
      'tagline': '',
      'primary_color': '#187A8F',
      'logo_url': '/files/a.png',
      'brand_assets': {'icon_512': '/files/b.png'},
    };
    bool needs(Map<String, Object?> now, {bool newLogo = false}) =>
        needsBrandRemake(saved, now, newLogo: newLogo);
    expect(needs(saved), isFalse);
    expect(needs(saved, newLogo: true), isTrue);
    expect(needs({...saved, 'name': '청소왕'}), isTrue);
    expect(needs({...saved, 'primary_color': '#00B386'}), isTrue);
    expect(needs({...saved, 'tagline': '새 소개'}), isTrue);
    expect(needs({...saved, 'brand_assets': null}), isTrue);
    expect(
      needs({...saved, 'logo_url': null, 'brand_assets': null}),
      isFalse,
      reason: 'no logo, nothing to make',
    );
  });
}
