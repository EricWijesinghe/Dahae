import 'package:core/core.dart';
import 'package:customer/screens/service.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/customer_host.dart';
import 'support/customer_routes.dart';

Service _cleaning({String? imageUrl}) => Service.fromJson({
  'code': 'studio',
  'pricing': {'type': 'fixed', 'minutes': 120, 'price': 45000},
  'content': {if (imageUrl != null) 'image_url': imageUrl},
});

Future<void> _pump(WidgetTester tester, Service s) async {
  setViewport(tester, phoneSize);
  final fake = FakeApi();
  stockCustomerApi(fake);
  final env = await TestEnv.create(fake: fake);
  await pumpCustomerScreen(tester, env, ServiceScreen(service: s));
  await settle(tester);
}

void main() {
  testWidgets('without a catalog photo the design placeholder card shows', (
    tester,
  ) async {
    await _pump(tester, _cleaning());

    expect(find.text(koT.servicePhotoSlot), findsOneWidget);
    expect(find.byIcon(Icons.image_outlined), findsOneWidget);
    expect(find.byType(Image), findsNothing);
    await unmount(tester);
  });

  testWidgets('a catalog photo replaces the placeholder', (tester) async {
    await _pump(tester, _cleaning(imageUrl: 'https://example.test/room.jpg'));

    // The network image is in the tree; a failed load falls back to the placeholder.
    expect(find.byType(Image), findsOneWidget);
    await unmount(tester);
  });
}
