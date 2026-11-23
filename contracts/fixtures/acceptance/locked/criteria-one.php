<?php

// invar-lock: SHOP-214 sha256:80333d7aa51c8bc9918facc4bbbc702861f3a1637111f880952725ca5a334cc4 ac=AC1

/**
 * AC1: Given 3 in stock, when I order 2, then 1 remains available.
 */
it('reserves stock (AC1)', function () {
    expect(true)->toBeFalse();
});
