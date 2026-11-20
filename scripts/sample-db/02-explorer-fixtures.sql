-- Objects for the database explorer: enum, domain, sequence, identity and
-- generated columns, unique/check constraints, expression index, functions,
-- a procedure, a trigger and a materialized view. Kept in their own schema
-- so the original sample tables are unchanged.

CREATE SCHEMA inventory;

CREATE TYPE inventory.stock_status AS ENUM ('in_stock', 'low', 'out_of_stock');
CREATE DOMAIN inventory.positive_qty AS integer CHECK (VALUE >= 0);
CREATE SEQUENCE inventory.sku_seq START 1000;

CREATE TABLE inventory.products (
    id              bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sku             text NOT NULL DEFAULT ('SKU-' || nextval('inventory.sku_seq')),
    name            text NOT NULL,
    price           numeric(10, 2) NOT NULL,
    quantity        inventory.positive_qty NOT NULL DEFAULT 0,
    status          inventory.stock_status NOT NULL DEFAULT 'in_stock',
    price_with_tax  numeric(10, 2) GENERATED ALWAYS AS (price * 1.2) STORED,
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT products_sku_key UNIQUE (sku),
    CONSTRAINT products_price_check CHECK (price >= 0)
);
COMMENT ON TABLE inventory.products IS 'Catalogue items';
COMMENT ON COLUMN inventory.products.price IS 'Unit price excluding tax';
CREATE INDEX products_lower_name_idx ON inventory.products (lower(name));

CREATE FUNCTION inventory.touch_updated_at() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

CREATE TRIGGER products_touch BEFORE UPDATE ON inventory.products
    FOR EACH ROW EXECUTE FUNCTION inventory.touch_updated_at();

CREATE FUNCTION inventory.stock_value(p_status inventory.stock_status DEFAULT NULL)
RETURNS numeric LANGUAGE sql STABLE AS $$
    SELECT coalesce(sum(price * quantity), 0)
    FROM inventory.products
    WHERE p_status IS NULL OR status = p_status
$$;
COMMENT ON FUNCTION inventory.stock_value(inventory.stock_status) IS 'Value of stock on hand';

CREATE PROCEDURE inventory.restock(p_id bigint, p_qty integer)
LANGUAGE sql AS $$
    UPDATE inventory.products SET quantity = quantity + p_qty WHERE id = p_id
$$;

INSERT INTO inventory.products (name, price, quantity, status) VALUES
    ('Keyboard', 49.00, 120, 'in_stock'),
    ('Mouse', 19.50, 8, 'low'),
    ('Monitor', 229.00, 0, 'out_of_stock'),
    ('Dock', 129.00, 35, 'in_stock');

CREATE MATERIALIZED VIEW inventory.stock_summary AS
SELECT status, count(*) AS products, sum(quantity) AS units
FROM inventory.products
GROUP BY status;

ANALYZE inventory.products;

GRANT USAGE ON SCHEMA inventory TO querylab_reader;
GRANT SELECT ON ALL TABLES IN SCHEMA inventory TO querylab_reader;
GRANT SELECT ON ALL SEQUENCES IN SCHEMA inventory TO querylab_reader;
