# pylint: disable=missing-docstring, bare-except, broad-except
# https://github.com/PyCQA/pylint/blob/5e0223bc82dca273590460a1114d2bfefaf62031/tests/functional/b/bad_except_order.py
__revision__ = 1

try:
    __revision__ += 1
except Exception:
    __revision__ = 0
except TypeError: # [bad-except-order]
    __revision__ = 0

try:
    __revision__ += 1
except LookupError:
    __revision__ = 0
except IndexError: # [bad-except-order]
    __revision__ = 0

try:
    __revision__ += 1
except (LookupError, NameError):
    __revision__ = 0
except (IndexError, UnboundLocalError): # [bad-except-order, bad-except-order]
    __revision__ = 0

try: # [bad-except-order]
    __revision__ += 1
except:
    pass
except Exception:
    pass

try:
    __revision__ += 1
except TypeError:
    __revision__ = 0
except:
    __revision__ = 0

try:
    __revision__ += 1
except Exception:
    pass
except:
    pass