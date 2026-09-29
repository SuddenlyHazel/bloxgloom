--!strict
return function(_host: BloxStartupHost): ()
    _host.register_texture("jade:tile", "tile")
    _host.register_item("jade:token", "Jade Token", "jade:tile")
end
