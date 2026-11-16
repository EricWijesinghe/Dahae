import 'package:core/core.dart';
import 'package:customer/screens/service.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'support/customer_host.dart';

void main() {
  testWidgets('the studio service card shows the 10평 limit note (design p7)', (
    tester,
  ) async {
    final env = await TestEnv.create(fake: FakeApi());
    await pumpCustomerScreen(
      tester,
      env,
      ServiceScreen(service: Service.fromJson(serviceJson('studio'))),
    );
    await settle(tester);

    expect(find.text(koT.dfStudioNote('10')), findsOneWidget);
    await unmount(tester);
  });
}
