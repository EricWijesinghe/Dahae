import 'package:core/core.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('formats an 11-digit mobile number as 3-4-4', () {
    expect(formatKoreanPhone('01012345678'), '010-1234-5678');
  });

  test('formats a 10-digit number as 3-3-4', () {
    expect(formatKoreanPhone('0112345678'), '011-234-5678');
  });

  test('drops existing hyphens and spaces before formatting', () {
    expect(formatKoreanPhone('010-1234 5678'), '010-1234-5678');
  });

  test('partial input formats as far as it can', () {
    expect(formatKoreanPhone('010'), '010');
    expect(formatKoreanPhone('0101'), '010-1');
    expect(formatKoreanPhone('01012345'), '010-123-45');
  });

  test('empty input stays empty', () {
    expect(formatKoreanPhone(''), '');
  });

  test('Seoul 02 numbers use a 2-digit area code', () {
    expect(formatKoreanPhone('0212345678'), '02-1234-5678');
    expect(formatKoreanPhone('021234567'), '02-123-4567');
  });

  test('+82 numbers are converted to the domestic form', () {
    expect(formatKoreanPhone('+82 10 1234 5678'), '010-1234-5678');
    expect(formatKoreanPhone('+82 2 1234 5678'), '02-1234-5678');
  });
}
