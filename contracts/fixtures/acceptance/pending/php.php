<?php

// invar-lock: SHOP-214

/**
 * AC1: Given 3 in stock, when I order 2, then 1 remains available.
 */
it('reserves stock (AC1)', function () {
    expect(true)->toBeFalse();
});
