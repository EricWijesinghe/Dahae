// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:customer/screens/service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';
import 'package:ui/ui.dart';

import 'support/customer_host.dart';
import 'support/customer_routes.dart';

Service _cleaning({String? imageUrl}) => Service.fromJson({
  'code': 'studio',
  'pricing': {'type': 'fixed', 'minutes': 120, 'price': 45000},
  'content': {'image_url': ?imageUrl},
});

/// Kitchen as the catalog can send it: fixed price, no photo, no preparation note.
Service _kitchen() => Service.fromJson({
  'code': 'kitchen',
  'pricing': {'type': 'fixed', 'minutes': 120, 'price': 45000},
  'needs_preparation': false,
  'content': <String, dynamic>{},
});

Future<void> _pump(
  WidgetTester tester,
  Service s, {
  Size size = phoneSize,
  String lang = 'ko',
}) async {
  setViewport(tester, size);
  final fake = FakeApi();
  stockCustomerApi(fake);
  final env = await TestEnv.create(fake: fake, localeCode: lang);
  await pumpCustomerScreen(tester, env, ServiceScreen(service: s));
  await settle(tester);
}

/// The catalog photo (Icon3D art is an asset Image, so match the network one only).
final _photo = find.byWidgetPredicate(
  (w) =>
      w is Image &&
      switch (w.image) {
        NetworkImage() => true,
        ResizeImage(:final imageProvider) => imageProvider is NetworkImage,
        _ => false,
      },
);

Finder _art(String code) => find.byWidgetPredicate(
  (w) => w is Icon3D && w.id == Icon3DRegistry.serviceId(code),
);

void main() {
  testWidgets('without a catalog photo the hero shows the service 3D art', (
    tester,
  ) async {
    await _pump(tester, _cleaning());

    expect(
      find.descendant(of: find.byType(ServiceHero), matching: _art('studio')),
      findsOneWidget,
    );
    expect(find.text('서비스 대표 사진 자리'), findsNothing);
    expect(find.byIcon(Icons.image_outlined), findsNothing);
    expect(_photo, findsNothing);
    await unmount(tester);
  });

  testWidgets('a catalog photo replaces the art', (tester) async {
    await _pump(tester, _cleaning(imageUrl: 'https://example.test/room.jpg'));

    // The network image is in the tree; a failed load falls back to the art.
    expect(_photo, findsOneWidget);
    await unmount(tester);
  });

  for (final lang in ['ko', 'en']) {
    testWidgets(
      '[$lang 360x780] kitchen shows no placeholder text and no empty heading',
      (tester) async {
        await _pump(tester, _kitchen(), size: const Size(360, 780), lang: lang);
        final t = lookupT(Locale(lang));

        expect(
          find.textContaining(RegExp('서비스 대표 사진 자리|Service photo goes here')),
          findsNothing,
        );
        expect(find.text(t.dfBasicService), findsNothing);
        expect(
          find.descendant(
            of: find.byType(ServiceHero),
            matching: _art('kitchen'),
          ),
          findsOneWidget,
        );
        expect(tester.takeException(), isNull);
        await unmount(tester);
      },
    );
  }

  testWidgets('the basic-service heading stays when its list is there', (
    tester,
  ) async {
    await _pump(tester, _cleaning());

    expect(find.text(koT.dfBasicService), findsOneWidget);
    expect(find.text(koT.dfScopeDusting), findsOneWidget);
    await unmount(tester);
  });
}
