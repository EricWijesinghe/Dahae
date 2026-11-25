// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter_test/flutter_test.dart';
import 'package:ui/ui.dart';

/// Pixel edge of the file's first frame.
Future<(int, int)> _size(String path) async {
  final codec = await ui.instantiateImageCodec(File(path).readAsBytesSync());
  final image = (await codec.getNextFrame()).image;
  final size = (image.width, image.height);
  image.dispose();
  codec.dispose();
  return size;
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('every registered id has art at 1x/2x/3x in exact pixel sizes', (
    tester,
  ) async {
    final wrong = <String>[];
    // Real image decoding needs the real event loop, not the fake-async clock.
    await tester.runAsync(() async {
      Future<void> check(String main, int px) async {
        for (final (d, k) in [('', 1), ('2.0x/', 2), ('3.0x/', 3)]) {
          final slash = main.lastIndexOf('/') + 1;
          final f = '${main.substring(0, slash)}$d${main.substring(slash)}';
          if (!File(f).existsSync()) {
            wrong.add('$f missing');
          } else if (await _size(f) != (px * k, px * k)) {
            wrong.add('$f is ${await _size(f)}, want ${px * k}');
          }
        }
      }

      for (final e in Icon3DRegistry.specs.entries) {
        await check(Icon3DRegistry.assetFor(e.key), 64);
        // Partner tabs without a grey file grey their active art instead.
        if (e.value.hasIdle && !e.key.startsWith('p_')) {
          await check(Icon3DRegistry.assetFor(e.key, idle: true), 64);
        }
        if (e.value.large) {
          await check(Icon3DRegistry.assetFor(e.key, size: 160), 160);
        }
      }
    });
    expect(wrong, isEmpty);
  });

  testWidgets('the success check animates: 24 distinct 288px frames', (
    tester,
  ) async {
    final seen = <String>{};
    await tester.runAsync(() async {
      for (var i = 0; i < SuccessMark.frameCount; i++) {
        final f = SuccessMark.frameAsset(i);
        expect(await _size(f), (288, 288), reason: f);
        seen.add(String.fromCharCodes(File(f).readAsBytesSync()));
      }
    });
    // Identical frames would freeze the success moment on one still image.
    expect(seen, hasLength(SuccessMark.frameCount));
  });
}
