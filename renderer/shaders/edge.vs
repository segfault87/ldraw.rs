#version 300 es

precision mediump float;

in vec3 position;
in vec3 color;

#ifdef USE_INSTANCING
    in vec4 instancedColor;
    in vec4 instancedEdgeColor;
    in mat4 instancedModelView;
#else
    uniform vec4 defaultColor;
    uniform vec4 edgeColor;
#endif

uniform mat4 projection;
uniform mat4 modelView;

out vec4 vColor;

void main(void) {
    vec4 pos4 = vec4(position, 1.0);
    mat4 transform = projection * modelView;
    #ifdef USE_INSTANCING
        transform *= instancedModelView;
    #endif
    gl_Position = transform * pos4;
    #ifdef USE_INSTANCING
        if (color.x < -1.0) {
            vColor = instancedEdgeColor;
        } else if (color.x < 0.0) {
            vColor = instancedColor;
        } else {
            vColor = vec4(color, 1.0);
        }
    #else
        if (color.x < -1.0) {
            vColor = edgeColor;
        } else if (color.x < 0.0) {
            vColor = defaultColor;
        } else {
            vColor = vec4(color, 1.0);
        }
    #endif
}
