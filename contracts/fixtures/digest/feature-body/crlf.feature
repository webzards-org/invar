  Scenario: Stock is reserved when an order is placed
    Given a Product with sku "ABC-123" and stock 10
    When a customer places an order for 3 units of "ABC-123"
    Then the Product stock for "ABC-123" is 7
