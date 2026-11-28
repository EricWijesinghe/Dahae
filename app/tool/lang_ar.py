# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Arabic: a machine-translated TEST locale for right-to-left layout (widget tests and goldens). It is not a user
# choice: LocaleController.names does not list it, so no app offers it.
# shortcut: only the keys below (and the strings_*.py X["ar"] rows) are Arabic; every other key reads English, so a
# test screen mixes both. Translate the rest and send it for review before Arabic ships as a choice.

import importlib.util
import pathlib

HERE = pathlib.Path(__file__).resolve().parent


def _english():
    out = {}
    for path in sorted(HERE.glob("strings_*.py")):
        spec = importlib.util.spec_from_file_location(path.stem, path)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        out.update({key: row[1] for key, row in mod.S.items()})
    return out


AR = {
  "tabHome": "الرئيسية",
  "tabBookings": "حجوزاتي",
  "tabChat": "المحادثة",
  "tabMore": "المزيد",
  "ok": "حسنًا",
  "yes": "نعم",
  "no": "لا",
  "next": "التالي",
  "save": "حفظ",
  "edit": "تعديل",
  "delete": "حذف",
  "close": "إغلاق",
  "retry": "أعد المحاولة",
  "cancel": "إلغاء",
  "logout": "تسجيل الخروج",
  "themeTip": "تغيير المظهر",
  "moreLicenses": "تراخيص المصادر المفتوحة",
  "cancelKeep": "احتفظ به",
  "cancelTitle": "هل تريد إلغاء هذا الحجز؟",
  "cancelled": "تم إلغاء الحجز",
  "doneTitle": "تم حجزك!",
  "doneBody": "سنرسل لك إشعارًا عند تعيين مدير.",
  "cancelFree": "ستسترد المبلغ كاملًا وتستعيد قسيمتك.",
  "cancelFeeNow": "الإلغاء الآن يفرض رسومًا قدرها {fee} ويعيد {refund}.",
  "cOpenBooking": "فتح الحجز",
  "cPaymentsTitle": "المدفوعات",
  "cSearchCommand": "ابحث أو انتقل",
  "cPaletteHint": "اكتب اسم قسم أو رقم حجز",
  "cSetupTitle": "استعد للإطلاق",
  "cSetupAllDone": "كل شيء جاهز!",
  "cSetupProgress": "تم {done} من {total}",
  "cSetupDevMode": "وضع الاختبار: لا تُرسل مدفوعات أو رسائل حقيقية",
  "cSetupSave": "حفظ",
  "cSetupSaved": "تم الحفظ",
  "cSetupShow": "إظهار",
  "cSetupHide": "إخفاء",
  "cSetupGuide": "طريقة الإعداد",
  "cSetupOptional": "اختياري",
  "cSetupClear": "قطع الاتصال",
  "cSetupTest": "تحقق من الاتصال",
  "cSetupNotConnected": "غير متصل",
  "cSetupOpenSite": "افتح الموقع",
  "cSetupModeLive": "مباشر",
  "cSetupModeTest": "اختبار",
  "cSetupLockedBadge": "مقفل",
  "cSetupUnlockedBadge": "غير مقفل",
  "cSetupLockNow": "اقفل الآن",
  "cSetupOtherMethods": "طرق دفع أخرى",
  "cSetupFromEnv": "يستخدم إعدادات الخادم",
  "cSetupKeepValue": "محفوظ (اكتب فقط للاستبدال)",
  "cSetupGatewayLive": "تذهب المدفوعات إلى: {name}",
  "cSetupGatewayTestKeys": "(مفاتيح اختبار: لا تنتقل أموال حقيقية)",
  "cSetupUnlockLeft": "يُقفل خلال {minutes} دقيقة",
  "cSetupOwnerOnly": "المالك وحده يستطيع فتح القفل",
  "cSetupUnlock": "فتح القفل (رمز المالك)",
  "cOwnerCodeWrong": "هذا الرمز غير صحيح. أدخل الرمز الذي يعرضه تطبيقك الآن",
  "cOwnerCodeLocked": "رموز خاطئة كثيرة. حاول مرة أخرى بعد بضع دقائق",
}

T = {**_english(), **AR}
