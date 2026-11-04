"""Daily editorial readiness depends on cards, without a synthetic issue summary."""
import unittest

import evening_preview


class EveningPreviewTests(unittest.TestCase):
    def test_valid_cards_do_not_require_an_overview(self):
        evening_preview.validate_ai_cards({'entries': [{'llm_used': True}]})

    def test_empty_or_source_fallback_cards_still_fail(self):
        for digest in [{}, {'entries': []}, {'entries': [{'llm_used': False}]},
                       {'entries': [{'llm_used': True}, {'description': 'Raw source'}]}]:
            with self.subTest(digest=digest), self.assertRaises(RuntimeError):
                evening_preview.validate_ai_cards(digest)


if __name__ == '__main__':
    unittest.main()
